use rimera_abi::RValue;

use crate::object::ManagedObject;

pub(crate) const MIN_COLLECTION_THRESHOLD: usize = 64 * 1024;

pub(crate) type HeapObject = ManagedObject;

#[derive(Debug)]
struct Slot {
    generation: u32,
    marked: bool,
    retired: bool,
    object: Option<HeapObject>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HeapStats {
    pub live: usize,
    pub slots: usize,
    pub free_slots: usize,
    pub retired_slots: usize,
    pub live_bytes: usize,
    pub peak_live_bytes: usize,
    pub collections: usize,
    pub next_collection_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionPhase {
    Idle,
    Mark,
    Lifecycle,
    Sweep,
}

#[derive(Debug)]
pub(crate) struct Heap {
    slots: Vec<Slot>,
    free: Vec<u32>,
    collections: usize,
    live_bytes: usize,
    peak_live_bytes: usize,
    retired_slots: usize,
    phase: CollectionPhase,
}

impl Default for Heap {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            collections: 0,
            live_bytes: 0,
            peak_live_bytes: 0,
            retired_slots: 0,
            phase: CollectionPhase::Idle,
        }
    }
}

impl Heap {
    fn current_live_bytes(&self) -> usize {
        self.slots.iter().fold(0_usize, |bytes, slot| {
            bytes.saturating_add(slot.object.as_ref().map_or(0, HeapObject::managed_size))
        })
    }

    pub(crate) fn refresh_managed_bytes(&mut self) {
        self.live_bytes = self.current_live_bytes();
        self.peak_live_bytes = self.peak_live_bytes.max(self.live_bytes);
    }

    pub(crate) fn allocate(&mut self, object: HeapObject) -> RValue {
        self.refresh_managed_bytes();
        let size = object.managed_size();
        let value = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.object.is_none());
            debug_assert!(!slot.retired);
            slot.object = Some(object);
            slot.marked = false;
            RValue::handle(index, slot.generation)
        } else {
            let index = u32::try_from(self.slots.len())
                .expect("Rimera heap cannot contain more than u32::MAX slots");
            self.slots.push(Slot {
                generation: 1,
                marked: false,
                retired: false,
                object: Some(object),
            });
            RValue::handle(index, 1)
        };
        self.live_bytes = self.live_bytes.saturating_add(size);
        self.peak_live_bytes = self.peak_live_bytes.max(self.live_bytes);
        value
    }

    pub(crate) fn get(&self, value: RValue) -> Option<&HeapObject> {
        let (index, generation) = value.handle_parts()?;
        let slot = self.slots.get(index as usize)?;
        (slot.generation == generation && !slot.retired)
            .then_some(slot.object.as_ref())
            .flatten()
    }

    pub(crate) fn get_mut(&mut self, value: RValue) -> Option<&mut HeapObject> {
        let (index, generation) = value.handle_parts()?;
        let slot = self.slots.get_mut(index as usize)?;
        (slot.generation == generation && !slot.retired)
            .then_some(slot.object.as_mut())
            .flatten()
    }

    pub(crate) fn live_values(&self) -> Vec<RValue> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.object.as_ref().map(|_| {
                    RValue::handle(
                        u32::try_from(index).expect("heap slot index must fit the handle ABI"),
                        slot.generation,
                    )
                })
            })
            .collect()
    }

    pub(crate) fn collect(&mut self, roots: impl IntoIterator<Item = RValue>) {
        debug_assert_eq!(self.phase, CollectionPhase::Idle);
        self.refresh_managed_bytes();
        self.collections += 1;
        self.phase = CollectionPhase::Mark;
        let mut worklist = roots.into_iter().collect::<Vec<_>>();
        while let Some(value) = worklist.pop() {
            let Some((index, generation)) = value.handle_parts() else {
                continue;
            };
            let Some(slot) = self.slots.get_mut(index as usize) else {
                continue;
            };
            if slot.generation != generation || slot.retired || slot.marked || slot.object.is_none()
            {
                continue;
            }
            slot.marked = true;
            if let Some(object) = &slot.object {
                object.trace_children(&mut |child| worklist.push(child));
            }
        }

        self.phase = CollectionPhase::Lifecycle;
        self.process_lifecycle_hooks();
        self.phase = CollectionPhase::Sweep;
        self.sweep();
        self.phase = CollectionPhase::Idle;
    }

    fn process_lifecycle_hooks(&mut self) {
        // A native bytearray export is an external lifetime obligation, not a
        // Python finalizer. Release unreachable views here, before sweep, so
        // a surviving bytearray can resize once its last view disappears.
        let exporters = self
            .slots
            .iter()
            .filter_map(|slot| {
                (!slot.marked)
                    .then_some(slot.object.as_ref())
                    .flatten()
                    .and_then(|object| match object {
                        HeapObject::MemoryView(view) if !view.released => Some(view.exporter),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        for exporter in exporters {
            if let Some(HeapObject::ByteArray(bytes)) = self.get_mut(exporter) {
                bytes.exports = bytes.exports.saturating_sub(1);
            }
        }
        // Weak-reference clearing and finalizer discovery belong between mark
        // and sweep. The phase remains the extension point for those later
        // Python behaviors.
    }

    fn sweep(&mut self) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(object) = slot.object.as_ref() else {
                continue;
            };
            if slot.marked {
                slot.marked = false;
                continue;
            }
            self.live_bytes = self.live_bytes.saturating_sub(object.managed_size());
            slot.object = None;
            if slot.generation == u32::MAX {
                slot.retired = true;
                self.retired_slots += 1;
            } else {
                slot.generation += 1;
                self.free.push(
                    u32::try_from(index).expect("existing heap slot index must fit the handle ABI"),
                );
            }
        }
    }

    pub(crate) fn live_bytes(&self) -> usize {
        self.current_live_bytes()
    }

    #[must_use]
    pub(crate) fn stats(&self, next_collection_bytes: usize) -> HeapStats {
        HeapStats {
            live: self
                .slots
                .iter()
                .filter(|slot| slot.object.is_some())
                .count(),
            slots: self.slots.len(),
            free_slots: self.free.len(),
            retired_slots: self.retired_slots,
            live_bytes: self.current_live_bytes(),
            peak_live_bytes: self.peak_live_bytes.max(self.current_live_bytes()),
            collections: self.collections,
            next_collection_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_generation_does_not_resolve() {
        let mut heap = Heap::default();
        let old = heap.allocate(HeapObject::String("old".to_owned()));
        heap.collect([]);
        let new = heap.allocate(HeapObject::String("new".to_owned()));
        assert!(heap.get(old).is_none());
        assert_eq!(heap.get(new), Some(&HeapObject::String("new".to_owned())));
    }

    #[test]
    fn rooted_parent_traces_its_child() {
        let mut heap = Heap::default();
        let child = heap.allocate(HeapObject::String("child".to_owned()));
        let parent = heap.allocate(HeapObject::ValueArray(vec![child].into_boxed_slice()));
        heap.collect([parent]);
        assert!(heap.get(parent).is_some());
        assert!(heap.get(child).is_some());
    }

    #[test]
    fn unreachable_object_graph_is_collected() {
        let mut heap = Heap::default();
        let child = heap.allocate(HeapObject::String("child".to_owned()));
        let parent = heap.allocate(HeapObject::ValueArray(vec![child].into_boxed_slice()));
        heap.collect([]);
        assert!(heap.get(parent).is_none());
        assert!(heap.get(child).is_none());
    }

    #[test]
    fn unreachable_cycle_is_collected() {
        let mut heap = Heap::default();
        let first = heap.allocate(HeapObject::ValueArray(Box::new([])));
        let second = heap.allocate(HeapObject::ValueArray(vec![first].into_boxed_slice()));
        let (first_index, _) = first.handle_parts().unwrap();
        heap.slots[first_index as usize].object =
            Some(HeapObject::ValueArray(vec![second].into_boxed_slice()));
        heap.collect([]);
        assert!(heap.get(first).is_none());
        assert!(heap.get(second).is_none());
    }

    #[test]
    fn unreachable_self_cycle_is_collected() {
        let mut heap = Heap::default();
        let value = heap.allocate(HeapObject::ValueArray(Box::new([])));
        let (index, _) = value.handle_parts().unwrap();
        heap.slots[index as usize].object =
            Some(HeapObject::ValueArray(vec![value].into_boxed_slice()));
        heap.collect([]);
        assert!(heap.get(value).is_none());
    }

    #[test]
    fn duplicate_roots_do_not_change_graph_reachability() {
        let mut heap = Heap::default();
        let child = heap.allocate(HeapObject::String("child".to_owned()));
        let parent = heap.allocate(HeapObject::ValueArray(vec![child].into_boxed_slice()));
        heap.collect([parent, parent, parent]);
        assert_eq!(heap.stats(MIN_COLLECTION_THRESHOLD).live, 2);
    }

    #[test]
    fn deep_graph_uses_an_iterative_worklist() {
        let mut heap = Heap::default();
        let mut root = heap.allocate(HeapObject::String("leaf".to_owned()));
        for _ in 0..100_000 {
            root = heap.allocate(HeapObject::ValueArray(vec![root].into_boxed_slice()));
        }
        heap.collect([root]);
        assert_eq!(heap.stats(MIN_COLLECTION_THRESHOLD).live, 100_001);
    }

    #[test]
    fn exhausted_generation_retires_the_slot() {
        let mut heap = Heap::default();
        let old = heap.allocate(HeapObject::String("old".to_owned()));
        let (index, _) = old.handle_parts().unwrap();
        heap.slots[index as usize].generation = u32::MAX;
        let exhausted = RValue::handle(index, u32::MAX);
        heap.collect([]);
        let replacement = heap.allocate(HeapObject::String("new".to_owned()));
        assert!(heap.get(exhausted).is_none());
        assert_ne!(replacement.handle_parts().unwrap().0, index);
        assert_eq!(heap.stats(MIN_COLLECTION_THRESHOLD).retired_slots, 1);
    }
}

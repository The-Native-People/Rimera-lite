use rimera_abi::RValue;

use crate::object::ManagedObject;

pub(crate) const MIN_COLLECTION_THRESHOLD: usize = 64 * 1024;

pub(crate) type HeapObject = ManagedObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifecycleAction {
    FinalizeBufferLease(RValue),
    CloseGenerator(RValue),
    FinalizeInstance(RValue),
    WeakReferenceCallback {
        weakref: RValue,
        callback: RValue,
        ordinal: u64,
    },
    Recollect,
}

impl LifecycleAction {
    pub(crate) fn roots(self) -> [Option<RValue>; 2] {
        match self {
            Self::FinalizeBufferLease(value)
            | Self::CloseGenerator(value)
            | Self::FinalizeInstance(value) => [Some(value), None],
            Self::WeakReferenceCallback {
                weakref, callback, ..
            } => [Some(weakref), Some(callback)],
            Self::Recollect => [None, None],
        }
    }
}

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
        self.allocate_cached(object)
    }

    /// Allocate using the byte total maintained by the heap itself instead of
    /// rescanning every live object first. Callers may use this only when
    /// mutable managed growth does not need an exact heap-limit check at this
    /// boundary; a collection refreshes the total before tracing/sweeping.
    pub(crate) fn allocate_cached(&mut self, object: HeapObject) -> RValue {
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

    #[cfg(test)]
    pub(crate) fn collect(
        &mut self,
        roots: impl IntoIterator<Item = RValue>,
    ) -> Vec<LifecycleAction> {
        self.collect_with_ephemerons(roots, &[])
    }

    /// Collects with weak-key/strong-value associations. An association keeps
    /// its value alive only when its key is reachable through ordinary roots
    /// (or another live association). This is used for context-owned metadata
    /// that must follow a managed owner's lifetime without pinning that owner.
    #[cfg(test)]
    pub(crate) fn collect_with_ephemerons(
        &mut self,
        roots: impl IntoIterator<Item = RValue>,
        associations: &[(RValue, RValue)],
    ) -> Vec<LifecycleAction> {
        self.collect_with_ephemerons_and_finalizers(roots, associations, &[])
    }

    pub(crate) fn collect_with_ephemerons_and_finalizers(
        &mut self,
        roots: impl IntoIterator<Item = RValue>,
        associations: &[(RValue, RValue)],
        finalizable: &[RValue],
    ) -> Vec<LifecycleAction> {
        debug_assert_eq!(self.phase, CollectionPhase::Idle);
        self.refresh_managed_bytes();
        self.collections += 1;
        self.phase = CollectionPhase::Mark;
        let worklist = roots.into_iter().collect::<Vec<_>>();
        self.mark_values(worklist);
        // Associations can form chains through values, so iterate to a fixed
        // point. The bound guarantees termination even if every pass marks one
        // additional key.
        for _ in 0..=associations.len() {
            let before = self.marked_count();
            for (key, value) in associations {
                if self.is_marked(*key) {
                    self.mark_values([*value].into());
                }
            }
            if self.marked_count() == before {
                break;
            }
        }

        self.phase = CollectionPhase::Lifecycle;
        let lifecycle_actions = self.process_lifecycle_hooks(finalizable);
        self.phase = CollectionPhase::Sweep;
        self.sweep();
        self.phase = CollectionPhase::Idle;
        lifecycle_actions
    }

    fn is_marked(&self, value: RValue) -> bool {
        let Some((index, generation)) = value.handle_parts() else {
            return false;
        };
        self.slots.get(index as usize).is_some_and(|slot| {
            slot.generation == generation && !slot.retired && slot.marked && slot.object.is_some()
        })
    }

    fn marked_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.marked).count()
    }

    fn mark_values(&mut self, mut worklist: Vec<RValue>) {
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
    }

    fn process_lifecycle_hooks(&mut self, finalizable: &[RValue]) -> Vec<LifecycleAction> {
        // Give an unreachable object with a pending Python finalizer one
        // protected turn before weak observations are cleared. RimeraContext
        // marks the instance finalized before invoking Python and the outer
        // collection loop then runs another mark/lifecycle/sweep turn.
        let finalizer_actions = finalizable
            .iter()
            .copied()
            .filter(|value| self.get(*value).is_some() && !self.is_marked(*value))
            .map(LifecycleAction::FinalizeInstance)
            .collect::<Vec<_>>();
        let pending_finalizers = finalizer_actions
            .iter()
            .filter_map(|action| match action {
                LifecycleAction::FinalizeInstance(value) => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();

        // Clear weak observations before any Python callback can run. Only a
        // reachable weak-reference object receives a callback; an unreachable
        // weak reference and its callback die together without observable work.
        // CPython orders callbacks for one referent from newest registration to
        // oldest, represented by the monotonic context-owned ordinal.
        let mut cleared_weakrefs = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let HeapObject::WeakReference(weakref) = slot.object.as_ref()? else {
                    return None;
                };
                let referent = slot.marked.then_some(weakref.referent).flatten()?;
                if pending_finalizers.contains(&referent) {
                    return None;
                }
                (!self.is_marked(referent)).then_some((
                    weakref.ordinal,
                    RValue::handle(
                        u32::try_from(index).expect("heap slot index must fit the handle ABI"),
                        slot.generation,
                    ),
                    weakref.callback,
                ))
            })
            .collect::<Vec<_>>();
        cleared_weakrefs.sort_unstable_by(|left, right| right.0.cmp(&left.0));
        let mut weakref_actions = Vec::new();
        let cleared_handles = cleared_weakrefs
            .iter()
            .map(|(_, weakref, _)| *weakref)
            .collect::<Vec<_>>();
        for (ordinal, weakref, callback) in cleared_weakrefs {
            if let Some(HeapObject::WeakReference(object)) = self.get_mut(weakref) {
                object.referent = None;
            }
            if let Some(callback) = callback {
                weakref_actions.push(LifecycleAction::WeakReferenceCallback {
                    weakref,
                    callback,
                    ordinal,
                });
            }
        }
        let mut pruned_container = false;
        for slot in &mut self.slots {
            if !slot.marked {
                continue;
            }
            let Some(HeapObject::WeakContainer(container)) = slot.object.as_mut() else {
                continue;
            };
            let before = container.entries.len();
            container
                .entries
                .retain(|entry| !cleared_handles.contains(&entry.weak));
            pruned_container |= container.entries.len() != before;
        }
        if pruned_container {
            weakref_actions.push(LifecycleAction::Recollect);
        }

        // First remove dead user-visible views from their shared PEP 688 lease.
        // A still-marked lease means another derived view remains alive.
        let leases_from_dead_views = self
            .slots
            .iter()
            .filter_map(|slot| {
                (!slot.marked)
                    .then_some(slot.object.as_ref())
                    .flatten()
                    .and_then(|object| match object {
                        HeapObject::MemoryView(view) if !view.released => view.lease,
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        for lease in leases_from_dead_views {
            if let Some(HeapObject::BufferLease(lease)) = self.get_mut(lease) {
                lease.active_views = lease.active_views.saturating_sub(1);
            }
        }

        // Python callbacks cannot run while the raw heap owns the collection
        // phase. Preserve each unreachable provider lease and its traced graph
        // for one turn; RimeraContext runs __release_buffer__, then collects
        // again to reclaim the now-finalized cycle.
        let mut lifecycle_actions = finalizer_actions;
        lifecycle_actions.extend(self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                if slot.marked {
                    return None;
                }
                match slot.object.as_ref() {
                    Some(HeapObject::BufferLease(lease)) if !lease.released => {
                        Some(LifecycleAction::FinalizeBufferLease(RValue::handle(
                            u32::try_from(index).expect("heap slot index must fit the handle ABI"),
                            slot.generation,
                        )))
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>());
        lifecycle_actions.extend(self.slots.iter().enumerate().filter_map(|(index, slot)| {
            if slot.marked {
                return None;
            }
            match slot.object.as_ref() {
                Some(HeapObject::Generator(generator))
                    if !generator.running
                        && !generator.closed
                        && !generator.completed
                        && !generator.finalizer_ran
                        && (generator.started
                            || generator.kind == crate::object::SuspendedKind::Coroutine) =>
                {
                    Some(LifecycleAction::CloseGenerator(RValue::handle(
                        u32::try_from(index).expect("heap slot index must fit the handle ABI"),
                        slot.generation,
                    )))
                }
                _ => None,
            }
        }));
        lifecycle_actions.extend(weakref_actions);
        self.mark_values(
            lifecycle_actions
                .iter()
                .copied()
                .flat_map(LifecycleAction::roots)
                .flatten()
                .collect(),
        );

        // Native bytearray export counting remains one obligation per native
        // memoryview. Provider-derived views carry a lease instead and do not
        // independently increment/decrement the backing exporter count.
        let exporters = self
            .slots
            .iter()
            .filter_map(|slot| {
                (!slot.marked)
                    .then_some(slot.object.as_ref())
                    .flatten()
                    .and_then(|object| match object {
                        HeapObject::MemoryView(view) if !view.released && view.lease.is_none() => {
                            Some(view.exporter)
                        }
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        for exporter in exporters {
            if let Some(HeapObject::ByteArray(bytes)) = self.get_mut(exporter) {
                bytes.exports = bytes.exports.saturating_sub(1);
            }
        }
        lifecycle_actions
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

    pub(crate) const fn cached_live_bytes(&self) -> usize {
        self.live_bytes
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
    use crate::object::{
        WeakContainerEntry, WeakContainerKind, WeakContainerObject, WeakReferenceObject,
    };

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
    fn ephemeron_value_lives_only_with_reachable_key() {
        let mut heap = Heap::default();
        let key = heap.allocate(HeapObject::String("key".to_owned()));
        let value = heap.allocate(HeapObject::String("value".to_owned()));
        heap.collect_with_ephemerons([key], &[(key, value)]);
        assert!(heap.get(key).is_some());
        assert!(heap.get(value).is_some());

        heap.collect_with_ephemerons([], &[(key, value)]);
        assert!(heap.get(key).is_none());
        assert!(heap.get(value).is_none());
    }

    #[test]
    fn ephemeron_chains_reach_a_fixed_point() {
        let mut heap = Heap::default();
        let first = heap.allocate(HeapObject::String("first".to_owned()));
        let second = heap.allocate(HeapObject::String("second".to_owned()));
        let third = heap.allocate(HeapObject::String("third".to_owned()));
        heap.collect_with_ephemerons([first], &[(second, third), (first, second)]);
        assert!(heap.get(first).is_some());
        assert!(heap.get(second).is_some());
        assert!(heap.get(third).is_some());
    }

    #[test]
    fn pending_finalizer_gets_one_protected_turn_before_weakref_clear() {
        let mut heap = Heap::default();
        let referent = heap.allocate(HeapObject::String("referent".to_owned()));
        let callback = heap.allocate(HeapObject::String("callback".to_owned()));
        let weakref = heap.allocate(HeapObject::WeakReference(WeakReferenceObject {
            referent: Some(referent),
            callback: Some(callback),
            cached_hash: None,
            proxy: false,
            callable_proxy: false,
            container_owned: false,
            ordinal: 1,
        }));

        assert_eq!(
            heap.collect_with_ephemerons_and_finalizers([weakref], &[], &[referent]),
            vec![LifecycleAction::FinalizeInstance(referent)]
        );
        assert!(heap.get(referent).is_some());
        assert!(matches!(
            heap.get(weakref),
            Some(HeapObject::WeakReference(object)) if object.referent == Some(referent)
        ));

        assert_eq!(
            heap.collect_with_ephemerons_and_finalizers([weakref], &[], &[]),
            vec![LifecycleAction::WeakReferenceCallback {
                weakref,
                callback,
                ordinal: 1,
            }]
        );
        assert!(heap.get(referent).is_none());
        assert!(matches!(
            heap.get(weakref),
            Some(HeapObject::WeakReference(object)) if object.referent.is_none()
        ));
    }

    #[test]
    fn reachable_weakrefs_clear_before_newest_first_callbacks() {
        let mut heap = Heap::default();
        let referent = heap.allocate(HeapObject::String("referent".to_owned()));
        let callback = heap.allocate(HeapObject::String("callback".to_owned()));
        let oldest = heap.allocate(HeapObject::WeakReference(WeakReferenceObject {
            referent: Some(referent),
            callback: Some(callback),
            cached_hash: None,
            proxy: false,
            callable_proxy: false,
            container_owned: false,
            ordinal: 1,
        }));
        let newest = heap.allocate(HeapObject::WeakReference(WeakReferenceObject {
            referent: Some(referent),
            callback: Some(callback),
            cached_hash: None,
            proxy: false,
            callable_proxy: false,
            container_owned: false,
            ordinal: 2,
        }));

        let actions = heap.collect([oldest, newest]);
        assert_eq!(
            actions,
            vec![
                LifecycleAction::WeakReferenceCallback {
                    weakref: newest,
                    callback,
                    ordinal: 2,
                },
                LifecycleAction::WeakReferenceCallback {
                    weakref: oldest,
                    callback,
                    ordinal: 1,
                },
            ]
        );
        assert!(heap.get(referent).is_none());
        for weakref in [oldest, newest] {
            assert!(matches!(
                heap.get(weakref),
                Some(HeapObject::WeakReference(object)) if object.referent.is_none()
            ));
        }
    }

    #[test]
    fn unreachable_weakref_does_not_keep_callback_or_emit_action() {
        let mut heap = Heap::default();
        let referent = heap.allocate(HeapObject::String("referent".to_owned()));
        let callback = heap.allocate(HeapObject::String("callback".to_owned()));
        let weakref = heap.allocate(HeapObject::WeakReference(WeakReferenceObject {
            referent: Some(referent),
            callback: Some(callback),
            cached_hash: None,
            proxy: false,
            callable_proxy: false,
            container_owned: false,
            ordinal: 1,
        }));

        assert!(heap.collect([]).is_empty());
        assert!(heap.get(referent).is_none());
        assert!(heap.get(callback).is_none());
        assert!(heap.get(weakref).is_none());
    }

    #[test]
    fn weak_container_pruning_requests_recollection_of_removed_payloads() {
        let mut heap = Heap::default();
        let referent = heap.allocate(HeapObject::String("key".to_owned()));
        let value = heap.allocate(HeapObject::String("value".to_owned()));
        let weak = heap.allocate(HeapObject::WeakReference(WeakReferenceObject {
            referent: Some(referent),
            callback: None,
            cached_hash: Some(1),
            proxy: false,
            callable_proxy: false,
            container_owned: false,
            ordinal: 1,
        }));
        let container = heap.allocate(HeapObject::WeakContainer(WeakContainerObject {
            kind: WeakContainerKind::KeyDictionary,
            entries: vec![WeakContainerEntry {
                weak,
                strong: Some(value),
                hash: 1,
            }],
            mutation_version: 0,
        }));

        assert_eq!(heap.collect([container]), vec![LifecycleAction::Recollect]);
        assert!(heap.get(referent).is_none());
        assert!(matches!(
            heap.get(container),
            Some(HeapObject::WeakContainer(object)) if object.entries.is_empty()
        ));
        assert!(heap.get(value).is_some());

        assert!(heap.collect([container]).is_empty());
        assert!(heap.get(value).is_none());
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

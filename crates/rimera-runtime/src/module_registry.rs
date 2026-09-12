use std::collections::{BTreeMap, btree_map::Entry};
use std::rc::Rc;

use rimera_abi::RNativeModuleInitializer;

/// A statically linked module description. Runtime module identity never lives
/// here: the authoritative cache remains the managed mapping exposed through
/// `sys.modules`.
#[derive(Debug, Clone)]
pub(crate) enum NativeModuleDefinition {
    Source {
        filename: String,
        package: String,
        is_package: bool,
        initializer: RNativeModuleInitializer,
    },
    Namespace {
        search_locations: Vec<String>,
    },
}

#[derive(Debug, Default)]
pub(crate) struct ModuleRegistry {
    definitions: BTreeMap<String, Rc<NativeModuleDefinition>>,
    resources: BTreeMap<String, BTreeMap<String, Box<[u8]>>>,
}

impl ModuleRegistry {
    pub(crate) fn register(
        &mut self,
        name: &str,
        definition: NativeModuleDefinition,
    ) -> Result<(), String> {
        match self.definitions.entry(name.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(Rc::new(definition));
                Ok(())
            }
            Entry::Occupied(_) => Err(format!("duplicate native module definition for '{name}'")),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<Rc<NativeModuleDefinition>> {
        self.definitions.get(name).map(Rc::clone)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }

    pub(crate) fn register_resource(
        &mut self,
        module: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let resources = self.resources.entry(module.to_owned()).or_default();
        match resources.entry(name.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(bytes.into());
                Ok(())
            }
            Entry::Occupied(_) => Err(format!("duplicate resource '{name}' for module '{module}'")),
        }
    }

    pub(crate) fn resource(&self, module: &str, name: &str) -> Option<&[u8]> {
        self.resources.get(module)?.get(name).map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModuleRegistry, NativeModuleDefinition};

    #[test]
    fn duplicate_registration_preserves_the_original_entries() {
        let mut registry = ModuleRegistry::default();
        registry
            .register(
                "pkg",
                NativeModuleDefinition::Namespace {
                    search_locations: vec!["first".to_owned()],
                },
            )
            .unwrap();
        assert!(
            registry
                .register(
                    "pkg",
                    NativeModuleDefinition::Namespace {
                        search_locations: vec!["replacement".to_owned()],
                    },
                )
                .is_err()
        );
        let definition = registry.get("pkg").unwrap();
        let NativeModuleDefinition::Namespace { search_locations } = definition.as_ref() else {
            panic!("expected namespace definition");
        };
        assert_eq!(search_locations, &["first"]);

        registry
            .register_resource("pkg", "data.bin", b"original")
            .unwrap();
        assert!(
            registry
                .register_resource("pkg", "data.bin", b"replacement")
                .is_err()
        );
        assert_eq!(
            registry.resource("pkg", "data.bin"),
            Some(b"original".as_slice())
        );
    }
}

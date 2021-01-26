mod impls;
mod inspectable_registry;
mod plugin;

pub use inspectable_registry::InspectableRegistry;
pub use plugin::WorldInspectorPlugin;

use bevy::ecs::{Location, ResourceRef};
use bevy::reflect::TypeRegistryArc;
use bevy::render::render_graph::base::MainPass;
use bevy::utils::{HashMap, HashSet};
use bevy::{ecs::TypeInfo, prelude::*};
use bevy_egui::egui;
use std::{any::TypeId, borrow::Cow};

/// Resource which controls the way the world inspector is shown.
#[derive(Debug)]
pub struct WorldInspectorParams {
    /// these components will be ignored
    pub ignore_components: HashSet<TypeId>,
}

struct WorldUIContext<'a> {
    world: &'a World,
    resources: &'a Resources,
    inspectable_registry: ResourceRef<'a, InspectableRegistry>,
    type_registry: ResourceRef<'a, TypeRegistryArc>,
    components: HashMap<Entity, (Location, Vec<TypeInfo>)>,
}
impl<'a> WorldUIContext<'a> {
    fn new(world: &'a World, resources: &'a Resources) -> WorldUIContext<'a> {
        let mut components: HashMap<Entity, (Location, Vec<TypeInfo>)> = HashMap::default();
        for (archetype_index, archetype) in world.archetypes().enumerate() {
            for (entity_index, entity) in archetype.iter_entities().enumerate() {
                let location = Location {
                    archetype: archetype_index as u32,
                    index: entity_index,
                };

                let entity_components = components
                    .entry(*entity)
                    .or_insert_with(|| (location, Vec::new()));

                assert_eq!(location.archetype, entity_components.0.archetype);
                assert_eq!(location.index, entity_components.0.index);

                entity_components.1.extend(archetype.types());
            }
        }

        let inspectable_registry = resources.get::<InspectableRegistry>().unwrap();
        let type_registry = resources.get::<TypeRegistryArc>().unwrap();

        WorldUIContext {
            world,
            resources,
            inspectable_registry,
            type_registry,
            components,
        }
    }

    fn components_of(&self, entity: Entity) -> impl Iterator<Item = (Location, &TypeInfo)> + '_ {
        let (location, types) = &self.components[&entity];
        types.iter().map(move |type_info| (*location, type_info))
    }

    fn entity_name(&self, entity: Entity) -> Cow<'_, str> {
        match self.world.get::<Name>(entity) {
            Ok(name) => name.as_str().into(),
            Err(_) => format!("Entity {}", entity.id()).into(),
        }
    }
}

impl WorldUIContext<'_> {
    fn ui(&self, ui: &mut egui::Ui, params: &WorldInspectorParams) {
        let root_entities = self.world.query_filtered::<Entity, Without<Parent>>();

        for entity in root_entities {
            self.entity_ui(ui, entity, params);
        }
    }

    fn entity_ui(&self, ui: &mut egui::Ui, entity: Entity, params: &WorldInspectorParams) {
        ui.collapsing(self.entity_name(entity), |ui| {
            ui.label("Components");

            for (location, type_info) in self.components_of(entity) {
                if params.should_ignore_component(type_info.id()) {
                    continue;
                }

                let type_name = type_info.type_name();
                let short_name = short_name(type_name);

                ui.collapsing(short_name, |ui| {
                    let could_display = self.inspectable_registry.generate(
                        self.world,
                        &self.resources,
                        location,
                        type_info,
                        &*self.type_registry.read(),
                        ui,
                    );

                    if !could_display {
                        ui.label("Inspectable has not been defined for this component");
                    }
                });
            }

            ui.separator();

            let children = self.world.get::<Children>(entity);
            if let Some(children) = children.ok() {
                ui.label("Children");
                for &child in children.iter() {
                    self.entity_ui(ui, child, params);
                }
            } else {
                ui.label("No children");
            }
        });
    }
}

impl WorldInspectorParams {
    /// Add `T` to component ignore list
    pub fn ignore_component<T: 'static>(&mut self) {
        self.ignore_components.insert(TypeId::of::<T>());
    }

    fn should_ignore_component(&self, type_id: TypeId) -> bool {
        self.ignore_components.contains(&type_id)
    }
}

impl Default for WorldInspectorParams {
    fn default() -> Self {
        let ignore_components = [
            TypeId::of::<Name>(),
            TypeId::of::<Children>(),
            TypeId::of::<Parent>(),
            TypeId::of::<PreviousParent>(),
            TypeId::of::<MainPass>(),
            TypeId::of::<Draw>(),
            TypeId::of::<RenderPipelines>(),
        ]
        .iter()
        .copied()
        .collect();

        WorldInspectorParams { ignore_components }
    }
}

fn short_name(type_name: &str) -> String {
    match type_name.find('<') {
        // no generics
        None => type_name.rsplit("::").next().unwrap_or(type_name).into(),
        // generics a::b::c<d>
        Some(angle_open) => {
            let angle_close = type_name.rfind('>').unwrap();

            let before_generics = &type_name[..angle_open];
            let after = &type_name[angle_close + 1..];
            let in_between = &type_name[angle_open + 1..angle_close];

            let before_generics = match before_generics.rfind("::") {
                None => before_generics,
                Some(i) => &before_generics[i + 2..],
            };

            let in_between = short_name(in_between);

            format!("{}<{}>{}", before_generics, in_between, after)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::short_name;

    #[test]
    fn shorten_name_basic() {
        assert_eq!(short_name("path::to::some::Type"), "Type".to_string());
    }
    #[test]
    fn shorten_name_generic() {
        assert_eq!(
            short_name("bevy::ecs::Handle<bevy::render::StandardMaterial>"),
            "Handle<StandardMaterial>".to_string()
        );
    }
    #[test]
    fn shorten_name_nested_generic() {
        assert_eq!(
            short_name("foo::bar::quux<qaax<p::t::b>>"),
            "quux<qaax<b>>".to_string()
        );
    }
}

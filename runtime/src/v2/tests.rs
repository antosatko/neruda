#![cfg(test)]

use crate::v2::ext::{EntitySpawnerBuilder, WorldBuilder};

#[test]
fn build() {
    let mut builder = WorldBuilder::new();

    let velocity = builder.add_static_component::<(f32, f32)>();
    let position = builder.add_dynamic_component::<(f32, f32)>();

    let player_flag = builder.add_flag_component();

    let mut world = builder.build();

    assert_eq!(1, world.static_components.len());
    assert_eq!(1, world.dynamic_components.len());
    assert_eq!(1, world.flag_components);

    let mut spwner = EntitySpawnerBuilder::new(&world)
        .with(position)
        .with(velocity)
        .build(&mut world);

    spwner.insert((0.1, 0.2));
    spwner.insert((1.0, 1.0));

    let entity_key = spwner.spawn(&mut world);
    let entity = *world.entities.get_unchecked(&entity_key);

    {
        let arch = world.archetypes.get_unchecked(&entity.archetype);
        assert_eq!(1, arch.entities.len());
    }
}

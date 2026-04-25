#![cfg(test)]

use crate::v2::ext::{EntitySpawnerBuilder, WorldBuilder};

#[test]
fn build() {
    let mut builder = WorldBuilder::new();

    let velocity = builder.add_static_component::<(f32, f32)>();
    let position = builder.add_dynamic_component::<(f32, f32)>();

    let player_flag = builder.add_flag_component();

    let q_velocity = builder.add_query(builder.new_query().with_include(velocity));

    let mut world = builder.build();

    assert_eq!(1, world.static_components.len());
    assert_eq!(1, world.dynamic_components.len());
    assert_eq!(1, world.flag_components);
    assert_eq!(0, world.unique_components.len());

    let mut spwner = EntitySpawnerBuilder::new(&world)
        .with(position)
        .with(velocity)
        .with_flag(player_flag)
        .build(&mut world);

    spwner.insert((0.1, 0.2));
    spwner.insert((1.0, 1.0));

    let entity_key = spwner.spawn(&mut world);

    world.iter_query(q_velocity, |world| {
        assert_eq!(world.entity(), entity_key);
        assert!(world.get_flag(player_flag));
        assert_ne!(world.get_static(velocity), &(1.0, 1.0))
    });
}

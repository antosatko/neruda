#![cfg(test)]

use crate::v2::{
    EntityRefKey,
    ext::{EntitySpawnerBuilder, WorldBuilder},
};

#[test]
fn build() {
    let mut builder = WorldBuilder::new();

    let velocity = builder.add_static_component::<(f32, f32)>();
    let position = builder.add_dynamic_component::<(f32, f32)>();

    let unit_flag = builder.add_flag_component();

    let camera_focus = builder.add_unique_component::<EntityRefKey>();

    let q_velocity = builder.add_query(builder.new_query().with_include(velocity));

    let mut world = builder.build();

    assert_eq!(1, world.static_components.len());
    assert_eq!(1, world.dynamic_components.len());
    assert_eq!(1, world.flag_components);
    assert_eq!(1, world.unique_components.len());

    let mut spwner = EntitySpawnerBuilder::new(&world)
        .with(velocity)
        .with(position)
        .with_flag(unit_flag)
        .build(&mut world);

    spwner.insert((0.1_f32, 0.2_f32));
    spwner.insert((1.0_f32, 1.0_f32));

    let entity_key = spwner.spawn(&mut world);

    world.iter_query(q_velocity, |world| {
        assert_eq!(world.entity(), entity_key);
        assert_eq!(world.get_static(velocity), &(0.1, 0.2));
        assert!(world.get_flag(unit_flag));
    });
}

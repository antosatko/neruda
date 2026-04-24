#![cfg(test)]

use crate::v2::ext::WorldBuilder;

#[test]
fn build() {
    let mut builder = WorldBuilder::new();

    let _velocity = builder.add_static_component::<(f32, f32)>();
    let _position = builder.add_static_component::<(f32, f32)>();

    let _player_flag = builder.add_flag_component();

    let world = builder.build();

    assert_eq!(2, world.static_components.len());
    assert_eq!(0, world.dynamic_components.len());
    assert_eq!(1, world.flag_components);
}

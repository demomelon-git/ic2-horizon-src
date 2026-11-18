fn ic2_be(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    crate::game::port_ic2::block_entity(block, entity);
}

fn ic2_be_ticks(block: &str, ticks: &mut bool) {
    crate::game::port_ic2::block_entity_ticks(block, ticks);
}

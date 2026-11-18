fn ic2_right_click(game: &mut crate::game::Game<'_>, pos: [i32; 3], face: [i32; 3], canceled: &mut bool) {
    let _ = face;
    crate::game::port_ic2::right_click(game, pos, canceled);
}

use guild_desktop::panel_corners::Geometry;

#[test]
fn controls_follow_column_and_left_dock_without_reserving_footer_space_for_docks() {
    for column in [None, Some(180.0), Some(280.0), Some(420.0)] {
        for left in [None, Some(200.0), Some(350.0)] {
            for width in [640.0, 1000.0, 1600.0] {
                let g = Geometry::new(width, column, left, 6.0);
                assert!(g.work_width >= 0.0);
                assert_eq!(g.column + g.work_width + 6.0, width);
                assert_eq!(
                    g.left,
                    column.unwrap_or(6.0) + left.map_or(0.0, |w| w + 6.0)
                );
            }
        }
    }
}

#[test]
fn resizing_a_dock_or_column_moves_the_anchor_by_the_same_distance() {
    let a = Geometry::new(1400.0, Some(240.0), Some(200.0), 6.0);
    let b = Geometry::new(1400.0, Some(300.0), Some(250.0), 6.0);
    assert_eq!(b.left - a.left, 110.0);
    assert_eq!(a.work_width - b.work_width, 60.0);
}

#[test]
fn bottom_tools_follow_the_surface_center_instead_of_branch_label_width() {
    let a = Geometry::new(1400.0, Some(232.0), None, 6.0);
    let b = Geometry::new(1600.0, Some(332.0), None, 6.0);
    assert_eq!(a.center(), a.column + a.work_width / 2.0);
    assert_eq!(b.center() - a.center(), 150.0);
}

#[test]
fn corrections_hidden_column_does_not_reserve_empty_control_space() {
    let g = Geometry::new(1400.0, None, None, 6.0);
    assert_eq!(g.left, 6.0);
    assert_eq!(g.work_width, 1388.0);
    let dock = Geometry::new(1400.0, None, Some(350.0), 6.0);
    assert_eq!(dock.work_width, 1388.0);
    assert_eq!(dock.center(), 700.0);
}

#[test]
fn corrections_controls_clamp_without_moving_the_panel() {
    let hidden = Geometry::new(1400.0, None, None, 6.0);
    assert_eq!(hidden.history_left(82.0), 82.0);
    assert_eq!(hidden.history_left(6.0), 6.0);
    assert_eq!(hidden.branch_left(), 6.0);
    let pinned = Geometry::new(1400.0, Some(232.0), Some(200.0), 6.0);
    assert_eq!(pinned.history_left(82.0), 82.0);
    assert_eq!(pinned.branch_left(), pinned.column);
}

#[test]
fn crowded_facts_raise_the_minimum_without_moving_the_panel_center() {
    let g = Geometry::new(1200.0, Some(232.0), Some(300.0), 6.0);
    let base = guild_desktop::frame::minimum(13.0, 154.0, 20.0);
    assert!(guild_desktop::frame::minimum(13.0, 154.0, 460.0).width > base.width);
    assert_eq!(g.center(), 713.0);
}

#[test]
fn branch_group_follows_the_left_dock_edge_and_only_moves_with_the_workspace_column() {
    let closed = Geometry::new(1400.0, Some(232.0), None, 6.0);
    let open = Geometry::new(1400.0, Some(232.0), Some(300.0), 6.0);
    let resized = Geometry::new(1400.0, Some(232.0), Some(420.0), 6.0);
    let column = Geometry::new(1400.0, Some(332.0), Some(420.0), 6.0);
    assert_eq!(closed.branch_left(), 232.0);
    assert_eq!(open.branch_left(), 232.0);
    assert_eq!(resized.branch_left(), 232.0);
    assert_eq!(column.branch_left(), 332.0);
    assert_eq!(closed.center(), 813.0);
    assert_eq!(resized.center(), closed.center());
    assert_eq!(column.center() - closed.center(), 50.0);
}

#[test]
fn footer_local_coordinates_keep_the_same_world_anchor_after_nesting() {
    let world = Geometry::new(1400.0, Some(232.0), Some(300.0), 6.0);
    let local = world.relative_to(232.0);
    assert_eq!(local.column, 0.0);
    assert_eq!(local.center() + 232.0, world.center());
    assert_eq!(local.work_width, world.work_width);
    let hidden = Geometry::new(1400.0, None, Some(300.0), 6.0).relative_to(0.0);
    assert_eq!(hidden.column, 6.0);
    assert_eq!(hidden.center(), 700.0);
}

#[test]
fn content_controls_follow_column_while_history_stays_fixed() {
    for column in [None, Some(180.0), Some(280.0), Some(420.0)] {
        for dock in [None, Some(200.0), Some(420.0)] {
            for window in [900.0, 1600.0] {
                let g = Geometry::new(window, column, dock, 6.0);
                assert_eq!(g.content_left(6.0), g.branch_left());
                assert_eq!(g.history_left(82.0), 82.0);
                assert_eq!(g.content_left(82.0), column.unwrap_or(6.0).max(82.0));
            }
        }
    }
}

#[test]
fn history_stays_next_to_the_workspace_toggle_when_column_changes() {
    for column in [None, Some(180.0), Some(420.0)] {
        let g = Geometry::new(1400.0, column, Some(300.0), 6.0);
        assert_eq!(g.history_left(108.0), 108.0);
    }
}

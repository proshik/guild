//! Offscreen Metal readback of the production DiffTab using only public APIs.
//! Requires an explicitly supplied isolated Git fixture; never drives a window.
use gpui::{AppContext, HeadlessAppContext, px, size};
use gpui_kit::test::TestWindowExt;
use guild_desktop::{
    diff_tab::{DiffTab, State},
    file_tab::Look,
    i18n::Ui,
    theme,
};
use std::{path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(
        std::env::var_os("GUILD_DIFF_RENDER_DIR").expect("explicit output directory"),
    );
    let repo = PathBuf::from(
        std::env::var_os("GUILD_DIFF_RENDER_REPO").expect("explicit isolated Git fixture"),
    );
    anyhow::ensure!(
        out.is_absolute() && repo.is_absolute(),
        "Use absolute task-owned paths"
    );
    std::fs::create_dir_all(&out)?;
    let revision = String::from_utf8(guild_desktop::git::run(&repo, &["rev-parse", "HEAD"])?)?
        .trim()
        .to_owned();
    let mut app = HeadlessAppContext::with_platform(
        gpui_kit::platform::current_platform(true).text_system(),
        Arc::new(guild_desktop::icons::Icons),
        gpui_kit::platform::current_headless_renderer,
    );
    app.update(|cx| {
        guild_desktop::fonts::register(cx);
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
    });
    let mut tab = None;
    let handle = app.open_window(size(px(1200.), px(800.)), |window, cx| {
        let view = cx.new(|cx| {
            DiffTab::open_file(
                &repo,
                "fixture.rs".into(),
                guild_session::Base::Commit(revision.clone()),
                guild_session::DiffView::SideBySide,
                None,
                Look {
                    family: "Menlo".into(),
                    font_size: 12.5,
                    theme: theme::Theme::dark(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                cx,
            )
        });
        tab = Some(view.clone());
        cx.new(|cx| gpui_component::Root::new(view, window, cx))
    })?;
    let tab = tab.unwrap();
    for _ in 0..400 {
        let loaded = app.update(|cx| {
            tab.update(cx, |t, cx| {
                t.poll(cx);
                matches!(t.files()[0].state, State::Hunks)
            })
        });
        app.run_until_parked();
        app.update_window(handle.into(), |_, window, cx| window.render_frame(cx))?;
        if loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    anyhow::ensure!(
        app.read_entity(&tab, |t, _| matches!(t.files()[0].state, State::Hunks)),
        "fixture failed to load"
    );
    let mut current = [false, false];
    for target in [[false, false], [true, false], [false, true], [true, true]] {
        let enabled = target == [true, true];
        let state_name = match target {
            [false, false] => "blame-off",
            [true, false] => "blame-old",
            [false, true] => "blame-new",
            _ => "blame-on",
        };
        for side in 0..2 {
            if current[side] == target[side] {
                continue;
            }
            app.update_window(handle.into(), |_, window, cx| {
                let id = (
                    if side == 0 {
                        "diff-blame-strip-old"
                    } else {
                        "diff-blame-strip-new"
                    },
                    1_usize,
                );
                if side == 0 {
                    window.click(id, cx);
                } else {
                    window.right_click(id, cx);
                }
                window.press("enter", cx);
            })?;
            current[side] = target[side];
            for _ in 0..10 {
                app.run_until_parked();
                app.update_window(handle.into(), |_, window, cx| window.render_frame(cx))?;
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        for _ in 0..4 {
            app.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.poll(cx);
                });
            });
            app.run_until_parked();
            app.update_window(handle.into(), |_, window, cx| window.render_frame(cx))?;
        }
        let name = format!("{state_name}.png");
        let image = app.capture_screenshot(handle.into())?;
        image.save(out.join(&name))?;
        // Actual GPU pixels, not a copy of the model's requested paint.
        let rows = app.read_entity(&tab, |t, _| {
            t.lines()
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    if let guild_desktop::diff_tab::Line::Source {
                        row:
                            guild_desktop::diff::document::DisplayRow::Text {
                                old,
                                new,
                                block: Some(block),
                            },
                        ..
                    } = line
                    {
                        Some((index, *block, old.is_some(), new.is_some()))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        let folds = app.read_entity(&tab, |t, _| {
            t.lines()
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    matches!(
                        line,
                        guild_desktop::diff_tab::Line::Source {
                            row: guild_desktop::diff::document::DisplayRow::Fold { .. },
                            ..
                        }
                    )
                    .then_some(index)
                })
                .collect::<Vec<_>>()
        });
        // Compare raster ink, not cell widths: identical one-/two-digit
        // labels must sit the same distance from the blame-facing edge.
        let numbered_rows = app.read_entity(&tab, |t, _| {
            t.lines()
                .iter()
                .enumerate()
                .filter_map(|(index, line)| {
                    if let guild_desktop::diff_tab::Line::Source {
                        row: guild_desktop::diff::document::DisplayRow::Text { old, new, .. },
                        ..
                    } = line
                    {
                        Some((index, *old, *new))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        let ink = app.update_window(handle.into(), |_, window, _| {
            let scale = image.width() as f32 / f32::from(window.viewport_size().width);
            [1_usize, 10].into_iter().map(|label| {
                let sides = [0, 1].map(|side| {
                    let (index, _, _) = numbered_rows.iter().find(|(_, old, new)| {
                        (if side == 0 { *old } else { *new }) == Some(label - 1)
                    }).expect("fixture contains the same one-/two-digit label on each side");
                    let cell = window.find((if side == 0 { "diff-numbers-old" } else { "diff-numbers-new" }, *index)).bounds();
                    let left = f32::from(cell.left()) * scale;
                    let right = f32::from(cell.right()) * scale;
                    let top = f32::from(cell.top()) * scale;
                    let bottom = f32::from(cell.bottom()) * scale;
                    let bg = image.get_pixel((left + 2. * scale) as u32, (top + 2. * scale) as u32).0;
                    let mut xs = Vec::new();
                    for y in (top + 2. * scale) as u32..(bottom - 2. * scale) as u32 {
                        for x in (left + 2. * scale) as u32..(right - 2. * scale) as u32 {
                            let p = image.get_pixel(x, y).0;
                            if (0..3).map(|c| p[c].abs_diff(bg[c])).max().unwrap() >= 30 { xs.push(x); }
                        }
                    }
                    let min = *xs.iter().min().expect("painted number ink") as f32;
                    let max = *xs.iter().max().unwrap() as f32 + 1.;
                    let gap = if side == 0 { right - max } else { min - left } / scale;
                    serde_json::json!({"side":side,"cell_x":[left/scale,right/scale],"ink_x":[min/scale,max/scale],"gap_to_blame_edge":gap})
                });
                let difference = (sides[0]["gap_to_blame_edge"].as_f64().unwrap() - sides[1]["gap_to_blame_edge"].as_f64().unwrap()).abs();
                serde_json::json!({"label":label,"sides":sides,"difference":difference,"pass":difference<=1.25})
            }).collect::<Vec<_>>()
        })?;
        std::fs::write(
            out.join(format!("{state_name}-number-ink.json")),
            serde_json::to_vec_pretty(&ink)?,
        )?;
        anyhow::ensure!(
            ink.iter().all(|sample| sample["pass"] == true),
            "number raster gaps must mirror at the blame-facing edges: {ink:?}"
        );
        let samples = app.update_window(handle.into(), |_, window, _| {
            let scale = image.width() as f32 / f32::from(window.viewport_size().width);
            let pixel = |x: gpui::Pixels, y: gpui::Pixels| {
                image
                    .get_pixel((f32::from(x) * scale) as u32, (f32::from(y) * scale) as u32)
                    .0
            };
            let mut kinds = std::collections::BTreeMap::new();
            for (index, block, old, new) in &rows {
                if kinds.contains_key(block) {
                    continue;
                }
                let side = usize::from(!*old);
                let code = window
                    .find((
                        if side == 0 {
                            "diff-old-code"
                        } else {
                            "diff-new-code"
                        },
                        *index,
                    ))
                    .bounds();
                let number = window
                    .find((
                        if side == 0 {
                            "diff-numbers-old"
                        } else {
                            "diff-numbers-new"
                        },
                        *index,
                    ))
                    .bounds();
                let y = code.top() + px(4.);
                let code_pixel = pixel(
                    if side == 0 {
                        code.right() - px(3.)
                    } else {
                        code.left() + px(3.)
                    },
                    y,
                );
                let number_pixel = pixel(
                    if side == 0 {
                        number.left() + px(3.)
                    } else {
                        number.right() - px(3.)
                    },
                    y,
                );
                assert_eq!(
                    code_pixel, number_pixel,
                    "changed block {block} must continue through number gutter (blame={enabled})"
                );
                kinds.insert(
                    *block,
                    serde_json::json!({"old":old,"new":new,"rgba":code_pixel,"scale":scale,"code_sample":[f32::from(if side==0 {code.right()-px(3.)}else{code.left()+px(3.)}),f32::from(y)],"number_sample":[f32::from(if side==0 {number.left()+px(3.)}else{number.right()-px(3.)}),f32::from(y)]}),
                );
            }
            let distinct = kinds
                .values()
                .map(|v| v["rgba"].to_string())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                distinct.len(),
                3,
                "insertion/deletion/modification must stay visibly distinct"
            );
            let body = window.find("diff-body").bounds();
            for side in 0..2 {
                let number = window
                    .find((
                        if side == 0 {
                            "diff-blame-strip-old"
                        } else {
                            "diff-blame-strip-new"
                        },
                        1_usize,
                    ))
                    .bounds();
                let x = if side == 0 {
                    number.left()
                } else {
                    number.right() - px(1.)
                };
                let y = body.bottom() - px(4.);
                assert_ne!(
                    pixel(x + px(0.5), y),
                    pixel(x + if side == 0 { px(-4.) } else { px(4.) }, y),
                    "separator continues below source rows"
                );
            }
            let divider=window.find("diff-divider-keyboard").bounds();
            let number=window.find(("diff-numbers-old",1_usize)).bounds();
            let base=pixel(number.left()+px(3.),number.top()+px(4.));
            let lane=pixel(divider.center().x,number.top()+px(4.));
            let below=pixel(divider.center().x,body.bottom()-px(4.));
            let mut layers=vec![serde_json::json!({"check":"common gutter background","actual":lane,"expected":base,"pass":lane==base}),serde_json::json!({"check":"center extends below text","actual":below,"expected":lane,"pass":below==lane})];
            assert!(!folds.is_empty(),"fixture must exercise a context fold");
            let header=window.find(("diff-row",0_usize)).bounds();
            for side in 0..2 {
                let number=window.find((if side==0 {"diff-blame-strip-old"}else{"diff-blame-strip-new"},1_usize)).bounds();
                let x=if side==0 {number.left()+px(0.5)}else{number.right()-px(0.5)};
                let y=header.top()+px(2.);
                layers.push(serde_json::json!({"check":"header masks rule","side":side,"actual":pixel(x,y),"expected":pixel(x+px(4.),y),"pass":pixel(x,y)==pixel(x+px(4.),y)}));
                for index in &folds {
                    let fold=window.find(("diff-fold",*index)).bounds();
                    let y=fold.top()+px(2.);
                    layers.push(serde_json::json!({"check":"fold masks rule","side":side,"actual":pixel(x,y),"expected":pixel(x+px(4.),y),"pass":pixel(x,y)==pixel(x+px(4.),y)}));
                }
            }
            std::fs::write(out.join(format!("{state_name}-layers.json")),serde_json::to_vec_pretty(&layers).unwrap()).unwrap();
            assert!(layers.iter().all(|layer|layer["pass"]==true),"gutter background/control masks: {layers:?}");
            if enabled {
                let index = rows
                    .iter()
                    .find(|(_, block, old, new)| *block == 1 && *old && *new)
                    .unwrap()
                    .0;
                let old = window.find(("diff-authors-old", index)).bounds();
                let new = window.find(("diff-authors-new", index)).bounds();
                let a = pixel(old.left() + px(2.), old.top() + px(4.));
                let b = pixel(new.left() + px(2.), new.top() + px(4.));
                assert_ne!(
                    a, b,
                    "two real blame timestamps must have different chronological shades"
                );
                assert!(
                    a[2] > a[1] && a[1] > a[0] && b[2] > b[1] && b[1] > b[0],
                    "annotation shading must be blue"
                );
            }
            kinds
        })?;
        std::fs::write(
            out.join(format!("{state_name}-pixels.json")),
            serde_json::to_vec_pretty(&samples)?,
        )?;
        let geometry=app.update_window(handle.into(),|_,window,_|{
            let old=window.find(("diff-blame-strip-old",1_usize));let new=window.find(("diff-blame-strip-new",1_usize));
            let rect=|b:gpui::Bounds<gpui::Pixels>|serde_json::json!({"x":f32::from(b.left()),"y":f32::from(b.top()),"width":f32::from(b.size.width),"height":f32::from(b.size.height),"click":[f32::from(b.center().x),f32::from(b.center().y)]});
            assert_eq!(old.bounds().size,new.bounds().size,"production strip dimensions are symmetric");
            for (side, enabled) in target.iter().enumerate() {assert_eq!(window.try_find((if side==0 {"diff-authors-old"}else{"diff-authors-new"},1_usize)).is_some(),*enabled);}
            serde_json::json!({"state":target,"coordinate_space":"offscreen DiffTab content/window logical points; not native screen coordinates","viewport":[f32::from(window.viewport_size().width),f32::from(window.viewport_size().height)],"old":{"label":old.label(),"bounds":rect(old.bounds())},"new":{"label":new.label(),"bounds":rect(new.bounds())},"divider":rect(window.find("diff-divider-keyboard").bounds())})
        })?;
        std::fs::write(
            out.join(format!("{state_name}-bounds.json")),
            serde_json::to_vec_pretty(&geometry)?,
        )?;
        println!(
            "OFFSCREEN production Rust DiffTab / Metal, not native acceptance: {}",
            out.join(&name).display()
        );
    }
    // Opaque fold backing must preserve its actual click/expansion behavior.
    let (fold, before) = app.read_entity(&tab, |t, _| {
        let fold = t
            .lines()
            .iter()
            .position(|line| {
                matches!(
                    line,
                    guild_desktop::diff_tab::Line::Source {
                        row: guild_desktop::diff::document::DisplayRow::Fold { .. },
                        ..
                    }
                )
            })
            .unwrap();
        let text = t
            .lines()
            .iter()
            .filter(|line| {
                matches!(
                    line,
                    guild_desktop::diff_tab::Line::Source {
                        row: guild_desktop::diff::document::DisplayRow::Text { .. },
                        ..
                    }
                )
            })
            .count();
        (fold, text)
    });
    app.update_window(handle.into(), |_, window, cx| {
        window.click(("diff-fold", fold), cx)
    })?;
    app.run_until_parked();
    let after = app.read_entity(&tab, |t, _| {
        t.lines()
            .iter()
            .filter(|line| {
                matches!(
                    line,
                    guild_desktop::diff_tab::Line::Source {
                        row: guild_desktop::diff::document::DisplayRow::Text { .. },
                        ..
                    }
                )
            })
            .count()
    });
    anyhow::ensure!(
        after > before,
        "backed context fold still expands through its real handler"
    );
    Ok(())
}

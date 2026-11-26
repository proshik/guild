//! Finite public DiffTab scheduling trial or marked isolated native Root.
#[cfg(feature = "native-preview")]
fn main() -> anyhow::Result<()> {
    use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
    use guild_desktop::{
        diff_tab::{DiffTab, State},
        file_tab::Look,
        i18n::Ui,
        native_preview::runtime_diff,
    };
    use std::{
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    let fixture =
        PathBuf::from(std::env::var_os("GUILD_DIFF_FIXTURE").expect("marked fixture path"));
    anyhow::ensure!(
        fixture.is_absolute() && fixture.join(".owned-runtime-diff").is_file(),
        "marked task-owned fixture required"
    );
    anyhow::ensure!(
        std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1"),
        "disable global hooks"
    );
    let native = std::env::var("GUILD_DIFF_NATIVE").as_deref() == Ok("1");
    let old = std::fs::read(fixture.join("old.txt"))?;
    let new = std::fs::read(fixture.join("new.txt"))?;
    anyhow::ensure!(
        old.len() as u64 <= guild_desktop::diff::DIFF_LIMIT
            && new.len() as u64 <= guild_desktop::diff::DIFF_LIMIT,
        "existing diff size limit"
    );
    gpui_kit::application().with_assets(guild_desktop::icons::Icons).run(move |cx| {
        guild_desktop::fonts::register(cx); guild_desktop::editor::init(cx); guild_desktop::actions::install(cx);
        guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 13., cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.open_window(WindowOptions {
            focus: native, show: native,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1200.),px(800.)),cx))),
            titlebar: Some(gpui::TitlebarOptions {title:Some("Guild isolated diff remediation".into()),..Default::default()}),
            ..Default::default()
        }, move |window,cx| {
            if native { let root=runtime_diff::root(&fixture,window,cx).unwrap(); return cx.new(|cx|gpui_component::Root::new(root,window,cx)); }
            let path=Path::new("fixture.txt");
            let begun=Instant::now();
            let tab=cx.new(|cx|DiffTab::open_proposal(&fixture,path.into(),Some(old.clone()),new.clone(),guild_session::DiffView::SideBySide,
                Look {family:"Menlo".into(),font_size:12.5,theme:Default::default(),autosave:false},Ui(guild_session::Language::English),cx));
            println!("{}",serde_json::json!({"constructor_ms":begun.elapsed().as_secs_f64()*1000.,"pid":std::process::id(),"visible":window.is_visible(),"active":window.is_window_active()}));
            let trial=tab.clone();
            cx.spawn(async move |cx| {
                let base=String::from_utf8(new).unwrap();
                for i in 0..6 {
                    let mut text=base.clone();
                    // Change only a fixed-width digit; keep exact input byte size.
                    text.replace_range(0..1, if i%2==0 {"A"} else {"B"});
                    let start=Instant::now();
                    cx.update(|cx| trial.update(cx,|t,cx|t.set_working(path.into(),Some(text.as_bytes().to_vec()),cx)));
                    let call_ms=start.elapsed().as_secs_f64()*1000.;
                    let mut polls=0;
                    let until=Instant::now()+Duration::from_secs(15);
                    loop {
                        cx.update(|cx| trial.update(cx,|t,cx|t.poll(cx)));
                        polls+=1;
                        if cx.update(|cx|runtime_diff::matches(&trial,path,&text,cx)) { break; }
                        assert!(Instant::now()<until,"diff never ready");
                        cx.background_executor().timer(Duration::from_millis(1)).await;
                    }
                    let state=cx.update(|cx|trial.read(cx).files()[0].state.clone());
                    assert!(matches!(state,State::Hunks));
                    println!("{}",serde_json::json!({"trial":i,"warmup":i==0,"bytes_each":text.len(),"update_ms":call_ms,"ready_ms":start.elapsed().as_secs_f64()*1000.,"ui_polls":polls,"correct":true}));
                }
                cx.update(|cx|cx.quit());
            }).detach();
            cx.new(|cx|gpui_component::Root::new(tab,window,cx))
        }).unwrap();
        if native { cx.activate(true); }
    });
    Ok(())
}
#[cfg(not(feature = "native-preview"))]
fn main() {
    eprintln!("use --features native-preview");
    std::process::exit(2);
}

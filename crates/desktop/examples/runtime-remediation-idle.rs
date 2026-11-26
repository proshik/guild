//! Finite actual Root/Book/LiveJobs idle reads; setup and invalidation are separate.
#[cfg(feature = "native-preview")]
fn main() {
    use gpui::{AppContext, WindowOptions};
    use guild_desktop::{native_preview::runtime_idle as driver, terminal_job::LiveJobs};
    use guild_session::terminal_job::{Book, JobReceipt, Presentation};
    use std::{
        hint::black_box,
        path::PathBuf,
        time::{Duration, Instant},
    };
    let fixture = PathBuf::from(std::env::var_os("GUILD_IDLE_FIXTURE").unwrap());
    assert!(fixture.is_absolute() && fixture.join(".owned-runtime-idle").is_file());
    assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref(), Ok("1"));
    gpui_kit::application().with_assets(guild_desktop::icons::Icons).run(move|cx|{
        guild_desktop::fonts::register(cx);guild_desktop::editor::init(cx);
        let window=cx.open_window(WindowOptions{show:false,focus:false,..Default::default()},|_,cx|cx.new(|_|gpui::Empty)).unwrap();
        cx.spawn(async move|cx|{
            println!("{}",serde_json::json!({"pid":std::process::id(),"protocol":"5 warmup, 100 reads; ns per call; finite provider-free models"}));
            for n in [1,20,100,500] {
                let root=window.update(cx,|_,window,cx|driver::documents(&fixture,n,window,cx)).unwrap();
                cx.background_executor().timer(Duration::from_millis(350)).await;
                cx.update(|cx|driver::flush(&root,cx));
                for _ in 0..5 {cx.update(|cx|driver::flush(&root,cx));}
                driver::observations();let mut times=Vec::new();
                for _ in 0..100 {let start=Instant::now();cx.update(|cx|driver::flush(&root,cx));times.push(start.elapsed().as_nanos() as u64);}
                println!("{}",serde_json::json!({"subject":"documents","entries":n,"ns":times,"observations":driver::observations()}));
                cx.update(|_|drop(root));cx.update(|_|{});
            }
            for n in [0,100,1000,10000] {
                let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(fixture.join(format!("receipts-{n}.json"))).unwrap()).unwrap();
                let rows:Vec<JobReceipt>=serde_json::from_value(saved["jobs"].clone()).unwrap();
                let mut book=Book::restore(rows).unwrap();
                let start=Instant::now();black_box(book.receipt_snapshot());let first=start.elapsed().as_nanos() as u64;
                for _ in 0..5 {black_box(book.receipt_snapshot());}
                let before=book.snapshot_build_count();let mut times=Vec::new();
                for _ in 0..100 {let start=Instant::now();let rows=book.receipt_snapshot();assert_eq!(rows.len(),n);black_box(&rows);drop(rows);times.push(start.elapsed().as_nanos() as u64);}
                let steady=book.snapshot_build_count()-before;
                let mut mutation=0;let mut first_after=0;let mut after_times=Vec::new();let mut after_builds=0;
                if let Some(id)=book.receipt_snapshot().first().map(|r|r.id) {
                    let before=book.snapshot_build_count();let start=Instant::now();book.get_mut(&id).unwrap().title="mutated once".into();mutation=start.elapsed().as_nanos() as u64;
                    let start=Instant::now();assert_eq!(book.receipt_snapshot().iter().find(|r|r.id==id).unwrap().title,"mutated once");first_after=start.elapsed().as_nanos() as u64;
                    for _ in 0..100 {let start=Instant::now();black_box(book.receipt_snapshot());after_times.push(start.elapsed().as_nanos() as u64);}
                    after_builds=book.snapshot_build_count()-before;
                }
                println!("{}",serde_json::json!({"subject":"book","entries":n,"first_ns":first,"ns":times,"steady_builds":steady,"mutation_ns":mutation,"first_after_ns":first_after,"after_ns":after_times,"after_builds_including_first":after_builds}));
                let path=guild_desktop::paths::state_dir().join(format!("receipts-{n}.json"));std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::copy(fixture.join(format!("receipts-{n}.json")),&path).unwrap();
                let mut jobs=LiveJobs::load(path);
                let start=Instant::now();black_box(jobs.receipt_snapshot());let first=start.elapsed().as_nanos() as u64;
                for _ in 0..5 {black_box(jobs.receipt_snapshot());}
                let before=jobs.snapshot_build_counts();let mut times=Vec::new();
                for _ in 0..100 {let start=Instant::now();let rows=jobs.receipt_snapshot();assert_eq!(rows.len(),n);black_box(&rows);drop(rows);times.push(start.elapsed().as_nanos() as u64);}
                let after=jobs.snapshot_build_counts();let steady=[after.0-before.0,after.1-before.1];
                let mut mutation=0;let mut first_after=0;let mut after_times=Vec::new();let mut after_builds=[0,0];
                if let Some(id)=jobs.receipt_snapshot().first().map(|r|r.id) {
                    let before=jobs.snapshot_build_counts();let start=Instant::now();jobs.present(&id,None,Presentation::Shown).unwrap();mutation=start.elapsed().as_nanos() as u64;
                    let start=Instant::now();assert_eq!(jobs.receipt_snapshot().iter().find(|r|r.id==id).unwrap().presentation,Presentation::Shown);first_after=start.elapsed().as_nanos() as u64;
                    for _ in 0..100 {let start=Instant::now();black_box(jobs.receipt_snapshot());after_times.push(start.elapsed().as_nanos() as u64);}
                    let after=jobs.snapshot_build_counts();after_builds=[after.0-before.0,after.1-before.1];
                }
                println!("{}",serde_json::json!({"subject":"livejobs","entries":n,"first_ns":first,"ns":times,"steady_builds":steady,"mutation_ns_includes_persistence":mutation,"first_after_ns":first_after,"after_ns":after_times,"after_builds_including_mutation_and_first":after_builds}));
            }
            println!("{}",serde_json::json!({"completed":true}));cx.update(|cx|cx.quit());
        }).detach();
    });
}
#[cfg(not(feature = "native-preview"))]
fn main() {
    eprintln!("use --features native-preview");
    std::process::exit(2);
}

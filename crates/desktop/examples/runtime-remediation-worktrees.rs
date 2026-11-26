//! Finite hidden production Root cadence, with setup excluded from accounting.
#[cfg(feature = "native-preview")]
fn main() {
    use gpui::{AppContext, WindowOptions};
    use guild_desktop::{git::worktrees::diagnostic, native_preview::runtime_worktrees as driver};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    fn cpu() -> [f64; 4] {
        // getrusage uses seconds/microseconds, independently of Mach tick sampling.
        unsafe {
            let mut own: libc::rusage = std::mem::zeroed();
            let mut children: libc::rusage = std::mem::zeroed();
            assert_eq!(libc::getrusage(libc::RUSAGE_SELF, &mut own), 0);
            assert_eq!(libc::getrusage(libc::RUSAGE_CHILDREN, &mut children), 0);
            let seconds = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
            [
                seconds(own.ru_utime),
                seconds(own.ru_stime),
                seconds(children.ru_utime),
                seconds(children.ru_stime),
            ]
        }
    }
    let fixture = PathBuf::from(std::env::var_os("GUILD_WORKTREES_FIXTURE").unwrap());
    let count: usize = std::env::var("GUILD_WORKTREES_ROOTS")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref(), Ok("1"));
    gpui_kit::application().with_assets(guild_desktop::icons::Icons).run(move|cx|{
        guild_desktop::fonts::register(cx);guild_desktop::editor::init(cx);
        cx.open_window(WindowOptions{show:false,focus:false,..Default::default()},move|window,cx|{
            let root=driver::root(&fixture,count,cx);let trial=root.clone();
            cx.spawn(async move|cx|{
                let setup=Instant::now();cx.update(|cx|driver::poll(&trial,cx));
                let mut fences=cx.update(|cx|driver::fence(&trial,cx));
                let mut last=diagnostic::counts();let mut stable=Instant::now();
                loop {
                    cx.update(|cx|driver::poll(&trial,cx));fences.retain(|rx|rx.try_recv().is_err());
                    let now=diagnostic::counts();if now!=last{stable=Instant::now();last=now;}
                    let ok=cx.update(|cx|driver::snapshot(&trial,cx)["status_ok"]==true);
                    if fences.is_empty()&&now.0>0&&now.0==now.1&&stable.elapsed()>Duration::from_secs(3)&&ok{break;}
                    assert!(setup.elapsed()<Duration::from_secs(240),"setup deadline");
                    cx.background_executor().timer(Duration::from_millis(20)).await;
                }
                println!("{}",serde_json::json!({"setup_seconds":setup.elapsed().as_secs_f64(),"setup_reads":last,"snapshot":cx.update(|cx|driver::snapshot(&trial,cx)),"pid":std::process::id()}));
                let initial=diagnostic::counts();let initial_cpu=cpu();let measured=Instant::now();
                cx.update(|cx|driver::begin(&trial,cx));
                println!("{}",serde_json::json!({"measurement":"begin","cpu_seconds":initial_cpu,"reads":initial}));
                let mut next=1;
                loop {
                    cx.update(|cx|driver::cadence(&trial,cx));
                    if measured.elapsed()>=Duration::from_millis(3000*next+800){
                        println!("{}",serde_json::json!({"wave":next,"elapsed_seconds":measured.elapsed().as_secs_f64(),"reads":diagnostic::counts(),"cpu_seconds":cpu()}));next+=1;
                        if next>20{break;}
                    }
                    cx.background_executor().timer(Duration::from_millis(20)).await;
                }
                let final_cpu=cpu();let final_reads=diagnostic::counts();
                assert_eq!(final_reads.0,final_reads.1,"all list invocations completed");
                println!("{}",serde_json::json!({"measurement":"end","elapsed_seconds":measured.elapsed().as_secs_f64(),"reads_delta":final_reads.0-initial.0,"cpu_delta_seconds":[final_cpu[0]-initial_cpu[0],final_cpu[1]-initial_cpu[1],final_cpu[2]-initial_cpu[2],final_cpu[3]-initial_cpu[3]],"snapshot":cx.update(|cx|driver::snapshot(&trial,cx))}));
                cx.update(|cx|driver::close(&trial,cx));
                cx.background_executor().timer(Duration::from_secs(2)).await;
                println!("{}",serde_json::json!({"cleanup":cx.update(|cx|driver::snapshot(&trial,cx))}));
                cx.update(|cx|cx.quit());
            }).detach();
            cx.new(|cx|gpui_component::Root::new(root,window,cx))
        }).unwrap();
    });
}
#[cfg(not(feature = "native-preview"))]
fn main() {
    eprintln!("use --features native-preview");
    std::process::exit(2);
}

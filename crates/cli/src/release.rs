//! `guild-release` — what `just release` asks of the changelog, M5.16.
//!
//! ```text
//! guild-release name <date> <short hash> <CHANGELOG.md>
//!                                            tags on stdin; the name on stdout
//! guild-release check <CHANGELOG.md>         something to release, or why not
//! guild-release cut <CHANGELOG.md> <version> <date>
//! ```
//!
//! Exit 0 when done, 1 on a refusal with its reason on stderr, 2 on a
//! call it does not understand. Everything it decides is
//! `guild_session::{changelog, update}`'s; this is the door recipes go through.

use std::io::Read;
use std::process::ExitCode;

use guild_session::changelog;
mod update_release;

const USAGE: &str = "usage: guild-release name <date> <short hash> <CHANGELOG.md> < tags\n       \
                     guild-release check <CHANGELOG.md>\n       \
                     guild-release cut <CHANGELOG.md> <version> <date>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match args.as_slice() {
        ["name", date, short, file] => name(date, short, file),
        ["check", file] => read(file).and_then(|text| {
            changelog::check(&text).map_err(|refusal| format!("refused: {refusal}"))
        }),
        ["cut", file, version, date] => cut(file, version, date),
        _ => {
            if let Some(result) = update_release::run(&args) {
                result
            } else {
                eprintln!(
                    "{USAGE}\n       guild-release build-info <output> <commit> <tag|-> <order|-> <clean|dirty> <built> <repository|-> <run|-> <attempt|->\n       guild-release stage <Guild.app> <destination>\n       guild-release select-tag <exact-tag|-> < tags\n       guild-release manifest <Guild.app> <zip> <owner/repo> <output-json>\n       guild-release sign <manifest> <zip> <private-key-path> <key-id> <new-output-directory>\n       guild-release verify <signed-directory> <trusted-public-keys-json>\n       guild-release keys-check <public-keys-json>\n       guild-release public-key <private-key-path> <key-id>"
                );
                return ExitCode::from(2);
            }
        }
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::from(1)
        }
    }
}

fn name(date: &str, short: &str, file: &str) -> Result<(), String> {
    let changelog = read(file)?;
    let mut tags = String::new();
    std::io::stdin()
        .read_to_string(&mut tags)
        .map_err(|err| format!("guild-release: the tags on stdin: {err}"))?;
    let tags: Vec<&str> = tags.lines().map(str::trim).collect();
    let name = changelog::version_name(date, short, &tags, &changelog).map_err(
        |changelog::Taken(name)| {
            format!("refused: {name} is a tag or a release of {file} already — made by hand?")
        },
    )?;
    println!("{name}");
    Ok(())
}

fn read(file: &str) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|err| format!("guild-release: {file}: {err}"))
}

fn cut(file: &str, version: &str, date: &str) -> Result<(), String> {
    let text = read(file)?;
    let released = changelog::release(&text, version, date)
        .map_err(|refusal| format!("refused: {refusal}"))?;
    std::fs::write(file, released).map_err(|err| format!("guild-release: {file}: {err}"))
}

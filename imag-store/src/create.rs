use std::path::PathBuf;
use std::io::stdin;
use std::fs::OpenOptions;
use std::result::Result as RResult;
use std::io::Read;
use std::ops::DerefMut;

use clap::ArgMatches;

use libimagrt::runtime::Runtime;
use libimagstore::store::Entry;
use libimagstore::store::EntryHeader;

use error::StoreError;
use error::StoreErrorKind;
use util::build_entry_path;
use util::build_toml_header;

type Result<T> = RResult<T, StoreError>;

pub fn create(rt: &Runtime) {
    rt.cli()
        .subcommand_matches("create")
        .map(|scmd| {
            debug!("Found 'create' subcommand...");

            // unwrap is safe as value is required
            let path = build_entry_path(rt, scmd.value_of("path").unwrap());
            debug!("path = {:?}", path);

            scmd.subcommand_matches("entry")
                .map(|entry| create_from_cli_spec(rt, scmd, &path))
                .ok_or(()) // hackythehackhack
                .map_err(|_| {
                    create_from_source(rt, scmd, &path)
                        .unwrap_or_else(|e| debug!("Error building Entry: {:?}", e))
                });
        });
}

fn create_from_cli_spec(rt: &Runtime, matches: &ArgMatches, path: &PathBuf) -> Result<()> {
    let content = matches.subcommand_matches("entry")
        .map(|entry_subcommand| {
            debug!("Found entry subcommand, parsing content");
            entry_subcommand
                .value_of("content")
                .map(String::from)
                .unwrap_or_else(|| {
                    entry_subcommand
                        .value_of("content-from")
                        .map(|src| string_from_raw_src(src))
                        .unwrap_or(String::new())
                })
        })
        .unwrap_or_else(|| {
            debug!("Didn't find entry subcommand, getting raw content");
            matches.value_of("from-raw")
                .map(|raw_src| string_from_raw_src(raw_src))
                .unwrap_or(String::new())
        });

    debug!("Got content with len = {}", content.len());

    rt.store()
        .create(PathBuf::from(path))
        .map(|mut element| {
            {
                let mut e_content = element.get_content_mut();
                *e_content = content;
                debug!("New content set");
            }
            {
                let mut e_header  = element.get_header_mut();
                matches.subcommand_matches("entry")
                    .map(|entry_matches| {
                        *e_header = build_toml_header(entry_matches, EntryHeader::new());
                        debug!("New header set");
                    });
            }
        })
        .map_err(|e| StoreError::new(StoreErrorKind::BackendError, Some(Box::new(e))))
}

fn create_from_source(rt: &Runtime, matches: &ArgMatches, path: &PathBuf) -> Result<()> {
    let content = matches
        .value_of("from-raw")
        .ok_or(StoreError::new(StoreErrorKind::NoCommandlineCall, None))
        .map(|raw_src| string_from_raw_src(raw_src));

    if content.is_err() {
        return content.map(|_| ());
    }
    let content = content.unwrap();
    debug!("Content with len = {}", content.len());

    Entry::from_str(path.clone(), &content[..])
        .map(|mut new_e| {
            rt.store()
                .create(path.clone())
                .map(|mut old_e| {
                    *old_e.deref_mut() = new_e;
                });

            debug!("Entry build");
        })
        .map_err(|serr| StoreError::new(StoreErrorKind::BackendError, Some(Box::new(serr))))
}

fn string_from_raw_src(raw_src: &str) -> String {
    let mut content = String::new();
    if raw_src == "-" {
        debug!("Reading entry from stdin");
        let res = stdin().read_to_string(&mut content);
        debug!("Read {:?} bytes", res);
    } else {
        debug!("Reading entry from file at {:?}", raw_src);
        OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(raw_src)
            .and_then(|mut f| f.read_to_string(&mut content));
    }
    content
}

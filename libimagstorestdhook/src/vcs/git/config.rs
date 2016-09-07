use toml::Value;

use libimagerror::into::IntoError;

use vcs::git::error::GitHookErrorKind as GHEK;
use vcs::git::error::MapErrInto;
use vcs::git::result::Result;

use vcs::git::action::StoreAction;

pub fn commit_interactive(config: &Value) -> bool {
    warn!("Interactive committing not yet supported, using dummy commit message");
    false
}

pub fn commit_message(config: &Value, action: StoreAction) -> String {
    if commit_interactive(config) {
        unimplemented!()
    } else {
        String::from("Dummy commit")
    }
}

pub fn abort_on_repo_init_err(cfg: Option<&Value>) -> bool {
    get_bool_cfg(cfg, "abort_on_repo_init_failure", true, true)
}

pub fn ensure_branch(cfg: Option<&Value>) -> Result<Option<String>> {
    match cfg {
        Some(cfg) => {
            match cfg.lookup("ensure_branch") {
                Some(&Value::String(ref s)) => Ok(Some(s.clone())),
                Some(_) => {
                    warn!("Configuration error, 'ensure_branch' must be a String.");
                    Err(GHEK::ConfigTypeError.into_error())
                        .map_err_into(GHEK::ConfigTypeError)
                },
                None => {
                    debug!("No key `ensure_branch'");
                    Ok(None)
                },
            }
        },
        None => Ok(None),
    }
}

pub fn do_checkout_ensure_branch(cfg: Option<&Value>) -> bool {
    get_bool_cfg(cfg, "try_checkout_ensure_branch", true, true)
}

fn get_bool_cfg(cfg: Option<&Value>, name: &str, on_fail: bool, on_unavail: bool) -> bool {
    cfg.map(|cfg| {
        match cfg.lookup(name) {
            Some(&Value::Boolean(b)) => b,
            Some(_) => {
                warn!("Configuration error, '{}' must be a Boolean (true|false).", name);
                warn!("Assuming '{}' now.", on_fail);
                on_fail
            },
            None => {
                debug!("No key '{}' - Assuming '{}'", name, on_unavail);
                on_unavail
            },
        }
    })
    .unwrap_or(on_unavail)
}


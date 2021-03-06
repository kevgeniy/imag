//
// imag - the personal information management suite for the commandline
// Copyright (C) 2015, 2016 Matthias Beyer <mail@beyermatthias.de> and contributors
//
// This library is free software; you can redistribute it and/or
// modify it under the terms of the GNU Lesser General Public
// License as published by the Free Software Foundation; version
// 2.1 of the License.
//
// This library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public
// License along with this library; if not, write to the Free Software
// Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301  USA
//

use std::path::PathBuf;
use std::fmt::{Debug, Formatter, Error as FmtError};
use std::result::Result as RResult;

use toml::Value;

use libimagerror::trace::trace_error;
use libimagstore::storeid::StoreId;
use libimagstore::hook::Hook;
use libimagstore::hook::result::HookResult;
use libimagstore::hook::accessor::{HookDataAccessor, HookDataAccessorProvider};
use libimagstore::hook::accessor::StoreIdAccessor;
use libimagutil::debug_result::*;

use vcs::git::error::GitHookErrorKind as GHEK;
use vcs::git::error::MapErrInto;
use vcs::git::runtime::Runtime as GRuntime;

pub struct StoreUnloadHook {
    storepath: PathBuf,

    runtime: GRuntime,
}

impl StoreUnloadHook {

    pub fn new(storepath: PathBuf) -> StoreUnloadHook {
        StoreUnloadHook {
            runtime: GRuntime::new(&storepath),
            storepath: storepath,
        }
    }

}

impl Debug for StoreUnloadHook {
    fn fmt(&self, fmt: &mut Formatter) -> RResult<(), FmtError> {
        write!(fmt, "StoreUnloadHook(storepath={:?}, repository={}, cfg={:?})",
               self.storepath,
               (if self.runtime.has_repository() { "Some(_)" } else { "None" }),
               self.runtime.has_config())
    }
}


impl Hook for StoreUnloadHook {

    fn name(&self) -> &'static str {
        "stdhook_git_storeunload"
    }

    /// Set the configuration of the hook. See
    /// `libimagstorestdhook::vcs::git::runtime::Runtime::set_config()`.
    ///
    /// This function traces the error (using `trace_error()`) that
    /// `libimagstorestdhook::vcs::git::runtime::Runtime::set_config()`
    /// returns, if any.
    fn set_config(&mut self, config: &Value) {
        if let Err(e) = self.runtime.set_config(config) {
            trace_error(&e);
        }
    }

}

impl HookDataAccessorProvider for StoreUnloadHook {

    fn accessor(&self) -> HookDataAccessor {
        HookDataAccessor::StoreIdAccess(self)
    }
}

impl StoreIdAccessor for StoreUnloadHook {

    fn access(&self, id: &StoreId) -> HookResult<()> {
        use libimagerror::into::IntoError;
        use vcs::git::action::StoreAction;
        use vcs::git::config::commit_message;
        use vcs::git::error::MapIntoHookError;
        use vcs::git::util::fetch_index;
        use vcs::git::config::abort_on_repo_init_err;
        use vcs::git::config::is_enabled;
        use vcs::git::config::committing_is_enabled;
        use vcs::git::config::add_wt_changes_before_committing;

        use git2::{ADD_DEFAULT,
                   StatusOptions,
                   Status,
                   StatusShow as STShow,
                   STATUS_INDEX_NEW as I_NEW,
                   STATUS_INDEX_DELETED as I_DEL,
                   STATUS_INDEX_RENAMED as I_REN,
                   STATUS_INDEX_MODIFIED as I_MOD,
                   STATUS_WT_NEW as WT_NEW,
                   STATUS_WT_DELETED as WT_DEL,
                   STATUS_WT_RENAMED as WT_REN,
                   STATUS_WT_MODIFIED as WT_MOD};

        let action = StoreAction::StoreUnload;
        let cfg    = try!(self.runtime.config_value_or_err(&action));

        if !is_enabled(cfg) {
            return Ok(())
        }

        if !self.runtime.has_repository() {
            debug!("[GIT STORE UNLOAD HOOK]: Runtime has no repository...");
            if try!(self.runtime.config_value_or_err(&action).map(|c| abort_on_repo_init_err(c))) {
                // Abort on repo init failure
                debug!("[GIT STORE UNLOAD HOOK]: Config says we should abort if we have no repository");
                debug!("[GIT STORE UNLOAD HOOK]: Returing Err(_)");
                return Err(GHEK::RepositoryInitError.into_error())
                    .map_err_into(GHEK::RepositoryError)
                    .map_into_hook_error()
            } else {
                debug!("[GIT STORE UNLOAD HOOK]: Config says it is okay to not have a repository");
                debug!("[GIT STORE UNLOAD HOOK]: Returing Ok(())");
                return Ok(())
            }
        }

        let _         = try!(self.runtime.ensure_cfg_branch_is_checked_out(&action));
        let repo      = try!(self.runtime.repository(&action));
        let mut index = try!(fetch_index(repo, &action));

        let check_dirty = |show: STShow, new: Status, modif: Status, del: Status, ren: Status| {
            let mut status_options = StatusOptions::new();
            status_options.show(show);
            status_options.include_untracked(true);

            repo.statuses(Some(&mut status_options))
                .map(|statuses| {
                    statuses.iter()
                        .map(|s| s.status())
                        .map(|s| {
                            debug!("STATUS_WT_NEW = {}",        s == new);
                            debug!("STATUS_WT_MODIFIED = {}",   s == modif);
                            debug!("STATUS_WT_DELETED = {}",    s == del);
                            debug!("STATUS_WT_RENAMED = {}",    s == ren);
                            s
                        })
                        .any(|s| s == new || s == modif || s == del || s == ren)
                })
                .map_err_into(GHEK::RepositoryStatusFetchError)
                .map_err_into(GHEK::RepositoryError)
                .map_into_hook_error()
        };

        if try!(check_dirty(STShow::Workdir, WT_NEW, WT_MOD, WT_DEL, WT_REN)) {
            if add_wt_changes_before_committing(cfg) {
                debug!("Adding WT changes before committing.");
                try!(index.add_all(&["*"], ADD_DEFAULT, None)
                    .map_err_into(GHEK::RepositoryPathAddingError)
                    .map_err_into(GHEK::RepositoryError)
                    .map_into_hook_error());
            } else {
                warn!("WT dirty, but adding files before committing on Drop disabled.");
                warn!("Continuing without adding changes to the index.");
            }
        } else {
            debug!("WT not dirty.");
        }

        if try!(check_dirty(STShow::Index, I_NEW, I_MOD, I_DEL, I_REN)) {
            debug!("INDEX DIRTY!");
        } else {
            debug!("INDEX CLEAN... not continuing!");
            return Ok(());
        }

        let signature = try!(
            repo.signature()
                .map_err_into(GHEK::MkSignature)
                .map_dbg_err_str("Failed to fetch signature")
                .map_dbg_str("[GIT STORE UNLOAD HOOK]: Fetched signature object")
                .map_into_hook_error()
        );

        let head = try!(
            repo.head()
                .map_err_into(GHEK::HeadFetchError)
                .map_dbg_err_str("Failed to fetch HEAD")
                .map_dbg_str("[GIT STORE UNLOAD HOOK]: Fetched HEAD")
                .map_into_hook_error()
        );

        let tree_id = try!(
            index.write_tree()
                .map_err_into(GHEK::RepositoryIndexWritingError)
                .map_dbg_err_str("Failed to write tree")
                .map_dbg_str("[GIT STORE UNLOAD HOOK]: Wrote index tree")
                .map_into_hook_error()
        );

        if !try!(committing_is_enabled(cfg)) {
            debug!("Committing not enabled. This is fine, returning now...");
            return Ok(())
        }

        let mut parents = Vec::new();
        {
            let commit = try!(
                repo.find_commit(head.target().unwrap())
                    .map_err_into(GHEK::RepositoryParentFetchingError)
                    .map_dbg_err_str("Failed to find commit HEAD")
                    .map_dbg_str("[GIT STORE UNLOAD HOOK]: Found commit HEAD")
                    .map_into_hook_error()
            );
            parents.push(commit);
        }

        // for converting from Vec<Commit> to Vec<&Commit>
        let parents = parents.iter().collect::<Vec<_>>();

        let tree = try!(
            repo.find_tree(tree_id)
                .map_err_into(GHEK::RepositoryParentFetchingError)
                .map_dbg_err_str("Failed to find tree")
                .map_dbg_str("[GIT STORE UNLOAD HOOK]: Found tree for index")
                .map_into_hook_error()
        );

        let message = try!(commit_message(&repo, cfg, action, &id)
                .map_dbg_err_str("Failed to get commit message")
                .map_dbg_str("[GIT STORE UNLOAD HOOK]: Got commit message"));

        try!(repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &parents)
            .map_dbg_str("Committed")
            .map_dbg_err_str("Failed to commit")
            .map_dbg_str("[GIT STORE UNLOAD HOOK]: Committed")
            .map_err_into(GHEK::RepositoryCommittingError)
            .map_into_hook_error()
        );

        index.write()
            .map_err_into(GHEK::RepositoryIndexWritingError)
            .map_dbg_err_str("Failed to write tree")
            .map_dbg_str("[GIT STORE UNLOAD HOOK]: Wrote index")
            .map_into_hook_error()
            .map(|_| ())
    }

}



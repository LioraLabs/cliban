//! Control-command router for exec requests (`ssh host <cmd>`).
//!
//! Pure sync: registry lookups only (blocking mutex, single-row queries),
//! no per-tenant store access — so it is unit-testable without SSH.

use chrono::Duration;
use cliban_core::time;
use cliban_tenancy::{Error as TenancyError, Role, Tenant, User};

use crate::config::SignupPolicy;
use crate::server::{AppState, KeyInfo};

/// How long minted invites stay redeemable.
pub const INVITE_TTL_DAYS: i64 = 7;

pub const USAGE: &str = "\
cliband commands (run as: ssh <host> <command>):
  signup <slug> [token]   create a tenant; enrolls your key as owner
  accept <code>           redeem an invite; enrolls your key as member
  whoami                  show your key, user, and memberships
  invite [slug]           mint a one-time invite code (owners only)
  members [slug]          list a tenant's members
";

/// Command result: text for the channel, exit status for the client.
pub struct Output {
    pub text: String,
    pub exit: u32,
}

fn ok(text: String) -> Output {
    Output { text, exit: 0 }
}

fn fail(text: String) -> Output {
    Output { text, exit: 1 }
}

fn tenancy_fail(cmd: &str, e: TenancyError) -> Output {
    // Domain errors have user-safe Display strings ("slug has already been
    // taken", "invite invalid or expired", ...). Infrastructure errors must
    // not leak internals (paths, schema) to remote clients: log server-side,
    // answer generically.
    match e {
        TenancyError::Sqlite(_) | TenancyError::Io(_) | TenancyError::Core(_) => {
            eprintln!("cliband: {cmd}: {e}");
            fail(format!("{cmd}: internal error\n"))
        }
        _ => fail(format!("{cmd}: {e}\n")),
    }
}

/// Run one control command for the authenticated key. `key` is mutated when
/// a command enrolls it (signup/accept) so later commands on the same
/// connection see the new identity.
pub fn run(state: &AppState, key: &mut KeyInfo, line: &str) -> Output {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["signup", slug] => signup(state, key, slug, None),
        ["signup", slug, token] => signup(state, key, slug, Some(token)),
        ["accept", code] => accept(state, key, code),
        ["whoami"] => whoami(state, key),
        ["invite"] => invite(state, key, None),
        ["invite", slug] => invite(state, key, Some(slug)),
        ["members"] => members(state, key, None),
        ["members", slug] => members(state, key, Some(slug)),
        _ => fail(USAGE.to_string()),
    }
}

/// Return the caller's registry user, enrolling the key (user + pubkey rows,
/// named after the sanitized SSH username) on first touch.
fn enroll(state: &AppState, key: &mut KeyInfo) -> Result<User, Output> {
    if let Some(u) = &key.user {
        return Ok(u.clone());
    }
    let name = display_name(&key.username);
    let user = state
        .manager
        .registry()
        .enroll_key(&name, &key.fingerprint, &key.openssh)
        .map_err(|e| tenancy_fail("enroll", e))?;
    key.user = Some(user.clone());
    Ok(user)
}

/// Display names come from the client-offered SSH username. Keep a
/// conservative charset (no control/ANSI bytes can reach other users'
/// terminals via `members`/`whoami`) and cap the length.
fn display_name(username: &str) -> String {
    let name: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(32)
        .collect();
    if name.is_empty() {
        "user".to_string()
    } else {
        name
    }
}

fn signup(state: &AppState, key: &mut KeyInfo, slug: &str, token: Option<&str>) -> Output {
    match (state.signup_policy, state.signup_token.as_deref()) {
        (SignupPolicy::Open, _) => {}
        (SignupPolicy::Closed, _) => return fail("signup: closed on this server\n".into()),
        // Policy "token" with no token configured: deny-all until the
        // operator sets signup_token (this is the shipped default config).
        (SignupPolicy::Token, None) => {
            return fail("signup: closed on this server (no signup token configured)\n".into())
        }
        (SignupPolicy::Token, Some(want)) => {
            if token != Some(want) {
                return fail("signup: invalid or missing signup token\n".into());
            }
        }
    }
    let user = match enroll(state, key) {
        Ok(u) => u,
        Err(out) => return out,
    };
    match state.manager.create_tenant(user.id, slug) {
        Ok(t) => ok(format!(
            "created tenant '{}'; your key is enrolled as owner\n",
            t.slug
        )),
        Err(e) => tenancy_fail("signup", e),
    }
}

fn whoami(state: &AppState, key: &KeyInfo) -> Output {
    let Some(user) = &key.user else {
        return ok(format!(
            "key {} is not enrolled\n\n{}",
            key.fingerprint, USAGE
        ));
    };
    let mut text = format!("user: {}\nkey: {}\n", user.name, key.fingerprint);
    match state.manager.registry().tenants_for_user(user.id) {
        Ok(ts) if ts.is_empty() => text.push_str("memberships: none\n"),
        Ok(ts) => {
            text.push_str("memberships:\n");
            for (t, role) in ts {
                text.push_str(&format!("  {} ({})\n", t.slug, role.as_str()));
            }
        }
        Err(e) => return tenancy_fail("whoami", e),
    }
    ok(text)
}

/// Resolve which tenant a command targets: an explicit slug must be one of
/// the caller's memberships; no slug is allowed only when the caller belongs
/// to exactly one tenant.
fn resolve_tenant(
    state: &AppState,
    user: &User,
    slug: Option<&str>,
    cmd: &str,
) -> Result<(Tenant, Role), Output> {
    let ts = state
        .manager
        .registry()
        .tenants_for_user(user.id)
        .map_err(|e| tenancy_fail(cmd, e))?;
    match slug {
        Some(s) => ts
            .into_iter()
            .find(|(t, _)| t.slug == s)
            .ok_or_else(|| fail(format!("{cmd}: you are not a member of '{s}'\n"))),
        None => match ts.len() {
            0 => Err(fail(format!("{cmd}: you have no tenant memberships\n"))),
            1 => Ok(ts.into_iter().next().expect("len checked")),
            _ => {
                let slugs: Vec<String> = ts.into_iter().map(|(t, _)| t.slug).collect();
                Err(fail(format!(
                    "{cmd}: you belong to several tenants, pass a slug: {}\n",
                    slugs.join(", ")
                )))
            }
        },
    }
}

fn accept(state: &AppState, key: &mut KeyInfo, code: &str) -> Output {
    // Enroll first (the registry needs a user id to redeem). A bad code
    // after enrollment leaves a known key with zero memberships — harmless,
    // and whoami reports it truthfully.
    let user = match enroll(state, key) {
        Ok(u) => u,
        Err(out) => return out,
    };
    match state.manager.registry().redeem_invite(code, user.id) {
        Ok(t) => ok(format!("joined tenant '{}' as member\n", t.slug)),
        Err(e) => tenancy_fail("accept", e),
    }
}

fn invite(state: &AppState, key: &KeyInfo, slug: Option<&str>) -> Output {
    let Some(user) = &key.user else {
        return fail(format!("invite: your key is not enrolled\n\n{USAGE}"));
    };
    let (tenant, role) = match resolve_tenant(state, user, slug, "invite") {
        Ok(v) => v,
        Err(out) => return out,
    };
    if role != Role::Owner {
        return fail(format!(
            "invite: only owners of '{}' can invite\n",
            tenant.slug
        ));
    }
    let expires = time::format_usec(time::now_usec() + Duration::days(INVITE_TTL_DAYS));
    match state.manager.registry().create_invite(&tenant.id, &expires) {
        Ok(inv) => ok(format!(
            "invite code for '{}': {}\nexpires: {}\nredeem with: ssh <host> accept {}\n",
            tenant.slug, inv.code, inv.expires_at, inv.code
        )),
        Err(e) => tenancy_fail("invite", e),
    }
}

fn members(state: &AppState, key: &KeyInfo, slug: Option<&str>) -> Output {
    let Some(user) = &key.user else {
        return fail(format!("members: your key is not enrolled\n\n{USAGE}"));
    };
    let (tenant, _role) = match resolve_tenant(state, user, slug, "members") {
        Ok(v) => v,
        Err(out) => return out,
    };
    match state.manager.registry().members(&tenant.id) {
        Ok(ms) => {
            let mut text = format!("members of '{}':\n", tenant.slug);
            for (u, role) in ms {
                text.push_str(&format!("  {} ({})\n", u.name, role.as_str()));
            }
            ok(text)
        }
        Err(e) => tenancy_fail("members", e),
    }
}

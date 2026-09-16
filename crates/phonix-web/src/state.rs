//! Server-side application state (SSR builds only).

use std::sync::Arc;

use axum::extract::FromRef;
use leptos::prelude::*;
use phonix_cache::Cache;
use phonix_config::AppConfig;
use phonix_core::{Error as CoreError, TenantSlug, TenantSummary};
use phonix_db::{Catalog, TenantRegistry};
use phonix_messaging::Publisher;
use phonix_services::{Caller, Hasher, SecretVault, Security};

/// Everything a request handler or server function may need.
///
/// Cheap to clone: every field is either an `Arc` or an internally reference-
/// counted handle (pools, connection managers, channels).
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub catalog: Catalog,
    pub tenants: TenantRegistry,
    pub cache: Cache,
    /// `None` when `rabbitmq.enabled = false`.
    pub publisher: Option<Publisher>,
    /// Argon2id at the configured cost. Built once: the parameters are fixed
    /// for the process, and a per-request one would re-derive them every time.
    pub hasher: Arc<Hasher>,
    /// Opens and seals TOTP secrets. Holds the key, so it is built at startup
    /// where a bad key kills the process rather than one user's enrolment.
    pub vault: Arc<SecretVault>,
    /// Where uploaded bytes live.
    ///
    /// A trait object, so nothing above this line can name a filesystem path -
    /// which is what makes swapping in an object store a change to one match
    /// arm in `startup` rather than a search of the whole application.
    pub storage: Arc<dyn phonix_storage::FileStorage>,
    /// How stored files are laid out beneath the tenant. Chosen once from
    /// configuration; see `phonix_storage::naming` for what the choice costs.
    pub naming: Arc<dyn phonix_storage::NamingStrategy>,
    pub leptos_options: LeptosOptions,
    /// How a server function tells the exporter that a request is waiting.
    ///
    /// A channel rather than a call, because the direction of the crates
    /// forbids the call: the worker is in `phonix-server`, which depends on
    /// this crate. `None` where nothing is listening - the tests, and any
    /// process running without background jobs - and then an export waits for
    /// the next poll instead, which is slower and not broken.
    pub exports: Option<ExportSignal>,
    /// What a browser sent to print a report is let in with.
    #[cfg(feature = "ssr")]
    pub printing: ExportTokens,
}

/// Somewhere to say that an export has been raised.
///
/// Unbounded, because the alternative is worse: a bounded channel would either
/// block the request that raised the export or drop the news of it, and what
/// flows through it is two ids. Losing one costs a wait for the next poll,
/// never the work - the row is in the database before anything is sent.
#[derive(Clone)]
#[cfg(feature = "ssr")]
pub struct ExportSignal(pub tokio::sync::mpsc::UnboundedSender<(TenantSlug, uuid::Uuid)>);

/// Nothing in the browser raises an export this way; it calls a server
/// function, which is what holds the sender.
#[derive(Clone)]
#[cfg(not(feature = "ssr"))]
pub struct ExportSignal;

/// The tokens a headless browser is let in with, while it prints.
///
/// # Why this is in memory and not a table
///
/// The exporter and the server that answers the browser are the same process,
/// and the token lives for as long as one print takes. A row would outlive the
/// process that minted it, which is the one thing a credential like this must
/// not do - and `user_tokens` is for secrets that travel in an email, not
/// between two halves of one binary.
///
/// It is not a session. It names one account for one address, it is withdrawn
/// the moment the print ends, and nothing renews it.
#[derive(Clone, Default)]
#[cfg(feature = "ssr")]
pub struct ExportTokens(Arc<std::sync::Mutex<std::collections::HashMap<String, Printing>>>);

/// What one outstanding token is good for.
#[cfg(feature = "ssr")]
pub struct Printing {
    tenant: TenantSlug,
    user: phonix_core::identity::UserId,
    /// Where the browser is sent, which is the only page this token opens.
    address: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(feature = "ssr")]
impl ExportTokens {
    /// Let a browser in, as this account, to read this one address.
    pub fn issue(
        &self,
        tenant: TenantSlug,
        user: phonix_core::identity::UserId,
        address: impl Into<String>,
        life: std::time::Duration,
    ) -> String {
        use secrecy::ExposeSecret;

        let token = phonix_services::crypto::token::IssuedToken::generate();
        let token = token.secret.expose_secret().to_owned();

        let printing = Printing {
            tenant,
            user,
            address: address.into(),
            expires_at: chrono::Utc::now()
                + chrono::TimeDelta::from_std(life).unwrap_or(chrono::TimeDelta::zero()),
        };

        if let Ok(mut held) = self.0.lock() {
            held.retain(|_, printing| printing.expires_at > chrono::Utc::now());
            held.insert(token.clone(), printing);
        }

        token
    }

    /// Where this token's browser is going, if it is still good for anywhere.
    pub fn address_of(&self, tenant: &TenantSlug, token: &str) -> Option<String> {
        self.with(tenant, token, |printing| printing.address.clone())
    }

    /// Who the browser holding this token is reading as.
    pub fn holder(
        &self,
        tenant: &TenantSlug,
        token: &str,
    ) -> Option<phonix_core::identity::UserId> {
        self.with(tenant, token, |printing| printing.user)
    }

    /// Take it back. Called when the print ends, however it ended.
    pub fn withdraw(&self, token: &str) {
        if let Ok(mut held) = self.0.lock() {
            held.remove(token);
        }
    }

    /// The tenant is checked here rather than by each caller: a token minted
    /// for one workspace must not open a page in another, whatever the host
    /// says.
    fn with<T>(
        &self,
        tenant: &TenantSlug,
        token: &str,
        read: impl FnOnce(&Printing) -> T,
    ) -> Option<T> {
        let held = self.0.lock().ok()?;
        let printing = held.get(token)?;

        (printing.tenant == *tenant && printing.expires_at > chrono::Utc::now())
            .then(|| read(printing))
    }
}

impl AppState {
    /// The bundle every identity use case takes.
    pub fn security(&self) -> Security<'_> {
        Security {
            config: &self.config.security,
            hasher: &self.hasher,
            vault: &self.vault,
        }
    }

    /// The bundle every file use case takes.
    ///
    /// Borrowed from the `Arc`s rather than cloning them: a use case runs
    /// inside one request and has no reason to outlive the state it was called
    /// from, and handing out owned handles would let one.
    pub fn files(&self) -> phonix_services::Files<'_> {
        phonix_services::Files {
            storage: self.storage.as_ref(),
            naming: self.naming.as_ref(),
        }
    }
}

// Lets axum hand `LeptosOptions` to the Leptos route handlers while the router
// carries the richer `AppState`.
impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}

/// Read [`AppState`] out of the Leptos context.
///
/// `phonix-server` provides it per request via `leptos_routes_with_context`.
pub fn app_state() -> Result<AppState, ServerFnError> {
    use_context::<AppState>().ok_or_else(|| {
        // Reaching this means the context was not provided during router setup,
        // which is a wiring bug rather than a bad request.
        ServerFnError::new(
            "AppState missing from the Leptos context; check leptos_routes_with_context",
        )
    })
}

/// Read the tenant that middleware attached to the current request.
pub async fn tenant_from_request() -> Result<TenantSummary, CoreError> {
    use axum::Extension;
    use leptos_axum::extract;

    // The extension is inserted by `phonix_server::middleware::resolve_tenant`,
    // which runs before any Leptos handler.
    let Extension(tenant): Extension<TenantSummary> = extract()
        .await
        .map_err(|_| CoreError::MissingTenantContext)?;

    Ok(tenant)
}

/// The tenant, or `None` on the bare domain.
///
/// Signup and the workspace picker both run on a host with no tenant, so the
/// absence of one is an ordinary state there rather than an error.
pub async fn optional_tenant() -> Option<TenantSummary> {
    tenant_from_request().await.ok()
}

/// The current tenant's connection pool.
pub async fn tenant_pool() -> Result<phonix_db::PgPool, ServerFnError> {
    let state = app_state()?;
    let tenant = tenant_from_request().await.map_err(ServerFnError::new)?;

    let handle = state
        .tenants
        .resolve(&tenant.slug)
        .await
        .map_err(|err| ServerFnError::new(CoreError::from(err)))?;

    Ok(handle.pool.clone())
}

/// The session cookie presented with this request, if any.
pub async fn session_token() -> Option<secrecy::SecretString> {
    use crate::server::cookie;

    let state = app_state().ok()?;
    let tenant = tenant_from_request().await.ok()?;
    let headers: http::HeaderMap = leptos_axum::extract().await.ok()?;

    let raw = headers.get(http::header::COOKIE)?.to_str().ok()?;
    let name = state
        .config
        .security
        .session
        .cookie_name_for(tenant.slug.as_str());

    cookie::read(raw, &name).map(secrecy::SecretString::from)
}

/// The print token this request carries, if it is a browser we sent.
#[cfg(feature = "ssr")]
pub async fn print_token() -> Option<String> {
    use crate::server::cookie;

    let state = app_state().ok()?;
    let tenant = tenant_from_request().await.ok()?;
    let headers: http::HeaderMap = leptos_axum::extract().await.ok()?;

    let raw = headers.get(http::header::COOKIE)?.to_str().ok()?;

    cookie::read(
        raw,
        &cookie::print_name(&state.config.security.session, tenant.slug.as_str()),
    )
}

/// Who is making this request.
///
/// Returns `None` when there is no session, an expired one, or an account that
/// has since been suspended. Every server function that changes something takes
/// the [`Caller`] this produces and states its permission - see
/// `phonix_services::caller`.
pub async fn current_caller() -> Result<Option<Caller>, ServerFnError> {
    let Some(token) = session_token().await else {
        return printing_caller().await;
    };

    let state = app_state()?;
    let pool = tenant_pool().await?;

    let auth_user = phonix_services::authenticate_session(&pool, &token, &state.config.security)
        .await
        .map_err(|err| ServerFnError::new(CoreError::from(err)))?;

    Ok(auth_user.map(Caller::user))
}

/// The account a browser we sent to print is reading as.
///
/// Not a session: the token names one account for one address and is withdrawn
/// when the print ends. It is resolved here rather than in a layer of its own
/// so that a page cannot be rendered for a printer by one path and refused to
/// the same account by another.
#[cfg(feature = "ssr")]
async fn printing_caller() -> Result<Option<Caller>, ServerFnError> {
    let Some(token) = print_token().await else {
        return Ok(None);
    };

    let state = app_state()?;
    let tenant = tenant_from_request().await.map_err(ServerFnError::new)?;

    let Some(user) = state.printing.holder(&tenant.slug, &token) else {
        return Ok(None);
    };

    let pool = tenant_pool().await?;
    let auth_user =
        phonix_services::identity::authentication::load_auth_user_by_id(&pool, user, true)
            .await
            .map_err(|err| ServerFnError::new(CoreError::from(err)))?;

    Ok(auth_user.map(Caller::user))
}

#[cfg(not(feature = "ssr"))]
async fn printing_caller() -> Result<Option<Caller>, ServerFnError> {
    Ok(None)
}

/// [`current_caller`], refusing anonymous requests.
///
/// For server functions where "not signed in" is a bug in the caller rather
/// than a state to render.
pub async fn require_caller() -> Result<Caller, ServerFnError> {
    current_caller()
        .await?
        .ok_or_else(|| ServerFnError::new(CoreError::Unauthenticated))
}

/// The tenant's pool and the signed-in caller, which most screens need together.
///
/// One helper rather than two calls because the two are always wanted at once
/// and resolving the caller already opened the pool.
pub async fn pool_and_caller() -> Result<(phonix_db::PgPool, Caller), ServerFnError> {
    let pool = tenant_pool().await?;
    let caller = require_caller().await?;
    Ok((pool, caller))
}

/// The state and tenant that inviting somebody needs.
///
/// One helper because the two are always wanted together, and because building
/// `Inviting` at each call site would mean each one deciding for itself where
/// the workspace slug comes from - and the slug is what makes the invitation
/// link point at a host that can set the session cookie.
pub async fn inviting_context() -> Result<(AppState, TenantSummary), ServerFnError> {
    let state = app_state()?;
    let tenant = tenant_from_request().await.map_err(ServerFnError::new)?;

    Ok((state, tenant))
}

/// The security policy this workspace has decided for itself.
///
/// Read per request rather than cached: an administrator tightening the MFA
/// policy expects it to take effect now, and this is one indexed row.
pub async fn workspace_settings() -> Result<phonix_core::WorkspaceSecuritySettings, ServerFnError> {
    let pool = tenant_pool().await?;
    phonix_services::workspace::settings::load(&pool)
        .await
        .map_err(service_error)
}

/// Collapse a service error into the coarse one a browser may see.
///
/// The full cause - a constraint name, a key description, a SQL fragment - is
/// logged inside the service layer and does not cross this boundary. Field
/// rejections survive, because a form needs them and they contain only what the
/// caller typed.
pub fn service_error(err: phonix_services::ServiceError) -> ServerFnError {
    ServerFnError::new(CoreError::from(err))
}

/// Set a `Set-Cookie` header on the response Leptos is building.
pub fn set_response_cookie(value: String) -> Result<(), ServerFnError> {
    let response = use_context::<leptos_axum::ResponseOptions>()
        .ok_or_else(|| ServerFnError::new("ResponseOptions missing from the Leptos context"))?;

    let header = http::HeaderValue::from_str(&value)
        .map_err(|_| ServerFnError::new("could not build the session cookie"))?;

    // `append`, not `insert`: signing in to a second workspace in the same
    // browser must not evict the first one's cookie.
    response.append_header(http::header::SET_COOKIE, header);
    Ok(())
}

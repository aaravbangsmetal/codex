mod device_code_auth;
mod dotenv_api_key;
mod onboard_oauth_helper;
mod pkce;
mod server;

pub use codex_client::BuildCustomCaTransportError as BuildLoginHttpClientError;
pub use device_code_auth::DeviceCode;
pub use device_code_auth::complete_device_code_login;
pub use device_code_auth::request_device_code;
pub use device_code_auth::run_device_code_login;
pub use server::LoginServer;
pub use server::ServerOptions;
pub use server::ShutdownHandle;
pub use server::run_login_server;

// Re-export commonly used auth types and helpers from codex-core for compatibility
pub use codex_app_server_protocol::AuthMode;
pub use codex_core::AuthManager;
pub use codex_core::CodexAuth;
pub use codex_core::auth::AuthDotJson;
pub use codex_core::auth::CLIENT_ID;
pub use codex_core::auth::CODEX_API_KEY_ENV_VAR;
pub use codex_core::auth::OPENAI_API_KEY_ENV_VAR;
pub use codex_core::auth::login_with_api_key;
pub use codex_core::auth::logout;
pub use codex_core::auth::save_auth;
pub use codex_core::token_data::TokenData;
pub use dotenv_api_key::upsert_dotenv_api_key;
pub use dotenv_api_key::validate_dotenv_target;
pub use onboard_oauth_helper::ApiProvisionOptions;
pub use onboard_oauth_helper::HelperError as OnboardOauthHelperError;
pub use onboard_oauth_helper::PendingApiProvisioning;
pub use onboard_oauth_helper::ProvisionedApiKey;
pub use onboard_oauth_helper::run_from_env as run_onboard_oauth_helper_from_env;
pub use onboard_oauth_helper::start_api_provisioning;

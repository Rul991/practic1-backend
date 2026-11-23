use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};
use crate::state::AppState;
use crate::error::AppError;

#[derive(Debug)]
pub struct CurrentUser { pub id: i64, pub _role: String }

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState)
        -> Result<Self, Self::Rejection>
    {
        let header = parts.headers.get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
        let claims = crate::jwt::decode(token, &state.config.jwt_secret)
            .map_err(|_| AppError::Unauthorized)?;
        Ok(CurrentUser { id: claims.sub, _role: claims.role })
    }
}
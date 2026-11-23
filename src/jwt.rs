use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub:  i64,
    pub role: String,
    pub exp:  i64,
}

pub fn encode(user_id: i64, role: &str, secret: &str, ttl: i64)
    -> anyhow::Result<(String, chrono::DateTime<Utc>)>
{
    let exp_at = Utc::now() + Duration::seconds(ttl);
    let claims = Claims { sub: user_id, role: role.into(), exp: exp_at.timestamp() };
    let token = jsonwebtoken::encode(
        &Header::default(), &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok((token, exp_at))
}

pub fn decode(token: &str, secret: &str) -> anyhow::Result<Claims> {
    let data = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
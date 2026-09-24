//! Contrato de sessão entre `marvyr-auth` (emite) e `marvyr-server` (valida).
//!
//! JWT HS256 com segredo compartilhado (`MARVYR_JWT_SECRET`). O game server
//! nunca vê senha: recebe só o token no `ClientHello` e resolve a conta pelo
//! `sub`. Validação é fail-closed — assinatura, expiração e emissor.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Emissor fixo: token de outro serviço com o mesmo segredo não vale aqui.
pub const ISSUER: &str = "marvyr-auth";
/// Segredo curto demais é recusado na partida dos dois serviços.
pub const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// Id da conta (`accounts.id`).
    pub sub: Uuid,
    /// Nome de capitão escolhido no registro.
    pub username: String,
    pub iss: String,
    /// Expiração (segundos Unix).
    pub exp: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenError {
    WeakSecret,
    Invalid,
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WeakSecret => write!(f, "segredo JWT com menos de {MIN_SECRET_LEN} bytes"),
            Self::Invalid => write!(f, "token de sessão inválido ou expirado"),
        }
    }
}

impl std::error::Error for TokenError {}

fn check_secret(secret: &[u8]) -> Result<(), TokenError> {
    if secret.len() < MIN_SECRET_LEN {
        return Err(TokenError::WeakSecret);
    }
    Ok(())
}

/// Emite um token para a conta, válido por `ttl_secs` a partir de `now`.
pub fn issue(
    account: Uuid,
    username: &str,
    secret: &[u8],
    now: u64,
    ttl_secs: u64,
) -> Result<String, TokenError> {
    check_secret(secret)?;
    let claims = Claims {
        sub: account,
        username: username.to_owned(),
        iss: ISSUER.to_owned(),
        exp: now + ttl_secs,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|_| TokenError::Invalid)
}

/// Valida assinatura, emissor e expiração.
pub fn verify(token: &str, secret: &[u8]) -> Result<Claims, TokenError> {
    check_secret(secret)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)
        .map(|data| data.claims)
        .map_err(|_| TokenError::Invalid)
}

/// Heurística barata para o servidor distinguir JWT de token anônimo de dev.
pub fn looks_like_jwt(token: &str) -> bool {
    token.split('.').count() == 3
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn issued_token_round_trips() {
        let account = Uuid::new_v4();
        let token = issue(account, "barba_ruiva", SECRET, now(), 3600).unwrap();
        assert!(looks_like_jwt(&token));
        let claims = verify(&token, SECRET).unwrap();
        assert_eq!(claims.sub, account);
        assert_eq!(claims.username, "barba_ruiva");
    }

    #[test]
    fn wrong_secret_expired_and_garbage_are_rejected() {
        let token = issue(Uuid::new_v4(), "x", SECRET, now(), 3600).unwrap();
        assert_eq!(
            verify(&token, b"ffffffffffffffffffffffffffffffff"),
            Err(TokenError::Invalid)
        );
        let expired = issue(Uuid::new_v4(), "x", SECRET, now() - 7200, 60).unwrap();
        assert_eq!(verify(&expired, SECRET), Err(TokenError::Invalid));
        assert_eq!(verify("a.b.c", SECRET), Err(TokenError::Invalid));
    }

    #[test]
    fn short_secret_is_refused() {
        assert_eq!(
            issue(Uuid::new_v4(), "x", b"curto", now(), 60),
            Err(TokenError::WeakSecret)
        );
    }
}

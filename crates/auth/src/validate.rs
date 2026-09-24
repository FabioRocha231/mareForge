//! Regras puras de credencial (sem I/O, testáveis isoladamente).

/// 3–20 caracteres ASCII `[A-Za-z0-9_]`. Unicidade é do banco (`lower(username)`).
pub fn username(name: &str) -> Result<(), &'static str> {
    if !(3..=20).contains(&name.len()) {
        return Err("nome de usuário deve ter entre 3 e 20 caracteres");
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("nome de usuário aceita apenas letras, números e _");
    }
    Ok(())
}

/// 8–128 caracteres (contados como `char`). O teto limita o custo do argon2.
pub fn password(password: &str) -> Result<(), &'static str> {
    if !(8..=128).contains(&password.chars().count()) {
        return Err("senha deve ter entre 8 e 128 caracteres");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_rules() {
        assert!(username("abc").is_ok());
        assert!(username("Capitao_Nemo_2026").is_ok());
        assert!(username(&"a".repeat(20)).is_ok());
        assert!(username("ab").is_err());
        assert!(username(&"a".repeat(21)).is_err());
        assert!(username("com espaco").is_err());
        assert!(username("capitão").is_err());
        assert!(username("a-b-c").is_err());
        assert!(username("").is_err());
    }

    #[test]
    fn password_rules() {
        assert!(password("12345678").is_ok());
        assert!(password(&"x".repeat(128)).is_ok());
        assert!(password("1234567").is_err());
        assert!(password(&"x".repeat(129)).is_err());
        // Multibyte conta por caractere, não por byte.
        assert!(password("çççççççç").is_ok());
    }
}

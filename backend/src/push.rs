use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder, WebPushClient,
    WebPushMessageBuilder,
};

use crate::error::{AppError, AppResult};
use crate::models::Settings;

#[derive(Debug, Deserialize)]
pub struct PushSubscriptionRequest {
    pub endpoint: String,
    pub keys: PushKeys,
}

#[derive(Debug, Deserialize)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Serialize)]
pub struct PushPayload {
    pub title: String,
    pub body: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

pub async fn ensure_vapid_keys(pool: &SqlitePool) -> AppResult<(String, String)> {
    let settings = sqlx::query_as::<_, Settings>("SELECT * FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;

    if !settings.vapid_public_key.is_empty() && !settings.vapid_private_key.is_empty() {
        return Ok((settings.vapid_public_key, settings.vapid_private_key));
    }

    let (public_b64, private_pem) = generate_vapid_keypair()?;

    sqlx::query(
        r#"
        UPDATE settings SET
            vapid_public_key = ?,
            vapid_private_key = ?,
            vapid_subject = CASE
                WHEN vapid_subject = '' OR vapid_subject = 'mailto:admin@localhost'
                THEN 'mailto:audiobooker@localhost'
                ELSE vapid_subject
            END,
            updated_at = datetime('now')
        WHERE id = 1
        "#,
    )
    .bind(&public_b64)
    .bind(&private_pem)
    .execute(pool)
    .await?;

    Ok((public_b64, private_pem))
}

fn generate_vapid_keypair() -> AppResult<(String, String)> {
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePrivateKey;
    use rand::rngs::OsRng;

    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let point = verifying_key.to_encoded_point(false);
    let public_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        point.as_bytes(),
    );
    let private_pem = signing_key
        .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
        .map_err(|e| AppError::internal(e.to_string()))?
        .to_string();
    Ok((public_b64, private_pem))
}

pub async fn save_subscription(
    pool: &SqlitePool,
    user_id: i64,
    sub: &PushSubscriptionRequest,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(endpoint) DO UPDATE SET
            user_id = excluded.user_id,
            p256dh = excluded.p256dh,
            auth = excluded.auth
        "#,
    )
    .bind(user_id)
    .bind(&sub.endpoint)
    .bind(&sub.keys.p256dh)
    .bind(&sub.keys.auth)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn notify_user(
    pool: &SqlitePool,
    settings: &Settings,
    user_id: i64,
    payload: &PushPayload,
) -> AppResult<()> {
    if settings.vapid_private_key.is_empty() || settings.vapid_public_key.is_empty() {
        return Ok(());
    }

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let body = serde_json::to_vec(payload).map_err(AppError::internal)?;
    let client = match IsahcWebPushClient::new() {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(error = %err, "web push client unavailable");
            return Ok(());
        }
    };

    for (endpoint, p256dh, auth) in rows {
        let subscription = SubscriptionInfo::new(&endpoint, &p256dh, &auth);
        let sig = match VapidSignatureBuilder::from_pem(
            settings.vapid_private_key.as_bytes(),
            &subscription,
        ) {
            Ok(mut builder) => {
                builder.add_claim("sub", settings.vapid_subject.clone());
                match builder.build() {
                    Ok(sig) => sig,
                    Err(err) => {
                        tracing::warn!(error = %err, "vapid signature failed");
                        continue;
                    }
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "vapid builder failed");
                continue;
            }
        };

        let mut builder = WebPushMessageBuilder::new(&subscription);
        builder.set_payload(ContentEncoding::Aes128Gcm, &body);
        builder.set_vapid_signature(sig);
        let message = match builder.build() {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(error = %err, "web push message build failed");
                continue;
            }
        };

        if let Err(err) = WebPushClient::send(&client, message).await {
            tracing::warn!(error = %err, "web push failed");
            let msg = err.to_string();
            if msg.contains("410") || msg.contains("404") {
                let _ = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
                    .bind(&endpoint)
                    .execute(pool)
                    .await;
            }
        }
    }

    Ok(())
}

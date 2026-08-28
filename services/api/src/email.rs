use crate::config::Config;
use anyhow::Context;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};

#[derive(Clone)]
pub enum EmailSender {
    Log {
        from: String,
    },
    Smtp {
        from: Mailbox,
        transport: AsyncSmtpTransport<Tokio1Executor>,
    },
}

impl EmailSender {
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        if config.email_driver == "log" {
            return Ok(Self::Log {
                from: config.email_from.clone(),
            });
        }
        let smtp_url = config
            .smtp_url
            .as_deref()
            .context("SMTP_URL is required when EMAIL_DRIVER=smtp")?;
        let from = config
            .email_from
            .parse::<Mailbox>()
            .context("EMAIL_FROM must be a valid mailbox")?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(smtp_url)
            .context("SMTP_URL is invalid")?
            .build();
        Ok(Self::Smtp { from, transport })
    }

    pub async fn send(&self, to: &str, subject: &str, action_url: &str) -> anyhow::Result<()> {
        match self {
            Self::Log { from } => {
                tracing::info!(to, subject, action_url, from, "development email");
                Ok(())
            }
            Self::Smtp { from, transport } => {
                let recipient = to
                    .parse::<Mailbox>()
                    .context("recipient email is invalid")?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(recipient)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(format!(
                        "{subject}\n\nOpen this link to continue:\n{action_url}\n"
                    ))
                    .context("could not build email")?;
                transport
                    .send(message)
                    .await
                    .context("SMTP delivery failed")?;
                Ok(())
            }
        }
    }
}

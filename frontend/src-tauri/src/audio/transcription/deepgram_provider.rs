// audio/transcription/deepgram_provider.rs
//
// Deepgram cloud transcription provider (fork addition).
//
// Each VAD speech segment from the live pipeline is sent to Deepgram's
// pre-recorded endpoint as a small in-memory WAV. Segments are short (a few
// seconds), so a request per segment keeps the existing worker architecture
// and still returns text within a second or two.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::warn;
use serde::Deserialize;
use std::time::Duration;

pub const DEFAULT_DEEPGRAM_MODEL: &str = "nova-3";
const DEEPGRAM_LISTEN_URL: &str = "https://api.deepgram.com/v1/listen";
const SAMPLE_RATE: u32 = 16_000;
/// 0.1 s at 16 kHz. Shorter clips are not worth a network round trip.
const MIN_SAMPLES: usize = 1_600;

pub struct DeepgramProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl DeepgramProvider {
    pub fn new(api_key: String, model: Option<String>) -> Result<Self, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Deepgram API key is missing. Add it in Settings > Transcription.".to_string());
        }
        let model = model
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_DEEPGRAM_MODEL.to_string());
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
        Ok(Self { api_key, model, client })
    }
}

/// Query string for one request. Without a language hint Deepgram detects it.
pub(crate) fn listen_query(model: &str, language: Option<&str>) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("model", model.to_string()),
        ("smart_format", "true".to_string()),
        ("punctuate", "true".to_string()),
    ];
    match language.map(str::trim).filter(|l| !l.is_empty() && *l != "auto") {
        Some(lang) => query.push(("language", lang.to_string())),
        None => query.push(("detect_language", "true".to_string())),
    }
    query
}

/// Encode 16 kHz mono f32 samples as a 16-bit PCM WAV file in memory.
pub(crate) fn encode_wav_pcm16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[derive(Debug, Deserialize)]
struct ListenResponse {
    results: Option<ListenResults>,
}

#[derive(Debug, Deserialize)]
struct ListenResults {
    #[serde(default)]
    channels: Vec<Channel>,
}

#[derive(Debug, Deserialize)]
struct Channel {
    #[serde(default)]
    alternatives: Vec<Alternative>,
}

#[derive(Debug, Deserialize)]
struct Alternative {
    #[serde(default)]
    transcript: String,
    confidence: Option<f32>,
}

/// Pull the best transcript out of a Deepgram /v1/listen response body.
pub(crate) fn parse_listen_response(body: &str) -> Result<TranscriptResult, TranscriptionError> {
    let parsed: ListenResponse = serde_json::from_str(body).map_err(|e| {
        TranscriptionError::EngineFailed(format!("Unexpected Deepgram response: {}", e))
    })?;
    let best = parsed
        .results
        .and_then(|r| r.channels.into_iter().next())
        .and_then(|c| c.alternatives.into_iter().next());
    Ok(match best {
        Some(alt) => TranscriptResult {
            text: alt.transcript.trim().to_string(),
            confidence: alt.confidence,
            is_partial: false,
        },
        None => TranscriptResult { text: String::new(), confidence: None, is_partial: false },
    })
}

/// Turn a non-success HTTP status into a message a user can act on.
pub(crate) fn describe_http_error(status: u16, body: &str) -> String {
    let hint = match status {
        401 | 403 => "Deepgram rejected the API key. Check it in Settings > Transcription.",
        402 => "Deepgram account has no remaining credit.",
        429 => "Deepgram rate limit reached; try again shortly.",
        s if s >= 500 => "Deepgram is having problems; try again shortly.",
        _ => "Deepgram request failed.",
    };
    let detail: String = body.chars().take(200).collect();
    format!("{} (HTTP {}: {})", hint, status, detail.trim())
}

#[async_trait]
impl TranscriptionProvider for DeepgramProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        let wav = encode_wav_pcm16(&audio, SAMPLE_RATE);
        let response = self
            .client
            .post(DEEPGRAM_LISTEN_URL)
            .query(&listen_query(&self.model, language.as_deref()))
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .body(wav)
            .send()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("Could not reach Deepgram: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| TranscriptionError::EngineFailed(format!("Reading Deepgram response failed: {}", e)))?;

        if !status.is_success() {
            let message = describe_http_error(status.as_u16(), &body);
            warn!("{}", message);
            return Err(TranscriptionError::EngineFailed(message));
        }
        parse_listen_response(&body)
    }

    async fn is_model_loaded(&self) -> bool {
        // Nothing to load locally; the key is validated on the first request.
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "Deepgram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_and_samples() {
        let wav = encode_wav_pcm16(&[0.0, 1.0, -1.0, 2.0], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        let pcm: Vec<i16> = wav[44..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(pcm, vec![0, i16::MAX, -i16::MAX, i16::MAX]); // 2.0 is clamped
    }

    #[test]
    fn query_uses_language_or_detection() {
        let q = listen_query("nova-3", Some("en"));
        assert!(q.contains(&("language", "en".to_string())));
        assert!(!q.iter().any(|(k, _)| *k == "detect_language"));
        for lang in [None, Some(""), Some("auto")] {
            let q = listen_query("nova-3", lang);
            assert!(q.contains(&("detect_language", "true".to_string())));
            assert!(q.contains(&("model", "nova-3".to_string())));
        }
    }

    #[test]
    fn parses_best_alternative() {
        let body = r#"{"metadata":{},"results":{"channels":[{"alternatives":[
            {"transcript":" We ship on Friday. ","confidence":0.97,"words":[]}]}]}}"#;
        let r = parse_listen_response(body).unwrap();
        assert_eq!(r.text, "We ship on Friday.");
        assert_eq!(r.confidence, Some(0.97));
        assert!(!r.is_partial);
    }

    #[test]
    fn empty_results_are_empty_text_not_errors() {
        let r = parse_listen_response(r#"{"results":{"channels":[]}}"#).unwrap();
        assert_eq!(r.text, "");
        assert!(parse_listen_response("not json").is_err());
    }

    #[test]
    fn http_errors_are_actionable() {
        assert!(describe_http_error(401, "{}").contains("API key"));
        assert!(describe_http_error(402, "").contains("credit"));
        assert!(describe_http_error(503, "").contains("try again"));
    }

    #[test]
    fn missing_key_is_rejected() {
        assert!(DeepgramProvider::new("  ".into(), None).is_err());
        let p = DeepgramProvider::new("k".into(), Some(" ".into())).unwrap();
        assert_eq!(p.model, DEFAULT_DEEPGRAM_MODEL);
    }
}

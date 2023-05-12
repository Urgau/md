//! InfoJson models

use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, marker::PhantomData, str::FromStr};

#[derive(Debug, Serialize, Deserialize)]
pub struct InfoJson {
    pub id: String,
    pub title: String,
    pub formats: Vec<Format>,
    pub thumbnails: Option<Vec<Thumbnail>>,
    pub thumbnail: Option<String>,
    pub description: Option<String>,
    pub uploader: Option<String>,
    pub uploader_id: Option<String>,
    pub uploader_url: Option<String>,
    pub channel_id: Option<String>,
    pub channel_url: Option<String>,
    pub duration: i64,
    pub view_count: Option<i64>,
    pub age_limit: Option<i64>,
    pub webpage_url: String,
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    // pub playable_in_embed: Option<bool>,
    // pub live_status: Option<String>,
    pub automatic_captions: Option<HashMap<String, Vec<AutomaticCaptionInfo>>>,
    pub subtitles: Option<HashMap<String, Vec<SubtitleInfo>>>,
    pub comment_count: Option<i64>,
    pub like_count: Option<i64>,
    pub channel: Option<String>,
    pub channel_follower_count: Option<i64>,
    pub upload_date: Option<String>,
    pub availability: Option<String>,
    // pub webpage_url_basename: String,
    // pub webpage_url_domain: String,
    pub extractor: String,
    pub extractor_key: String,
    pub display_id: String,
    pub fulltitle: String,
    pub duration_string: String,
    pub is_live: Option<bool>,
    pub was_live: Option<bool>,
    pub format: String,
    pub format_id: String,
    pub ext: String,
    pub protocol: String,
    pub format_note: Option<String>,
    pub filesize_approx: i64,
    pub tbr: f64,
    pub width: i64,
    pub height: i64,
    pub resolution: String,
    pub fps: Option<f64>,
    pub dynamic_range: Option<String>,
    #[serde(deserialize_with = "lit_none_string")]
    pub vcodec: Option<String>,
    pub vbr: f64,
    pub aspect_ratio: Option<f64>,
    #[serde(deserialize_with = "lit_none_string")]
    pub acodec: Option<String>,
    pub abr: Option<f64>,
    pub asr: Option<i64>,
    pub audio_channels: Option<i64>,
    pub epoch: i64,
    #[serde(rename = "_type")]
    pub info_json_type: String,
    #[serde(rename = "_version")]
    pub version: Version,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutomaticCaptionInfo {
    pub ext: String,
    pub url: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtitleInfo {
    pub ext: String,
    pub url: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Format {
    pub format_id: String,
    pub format_note: Option<String>,
    pub ext: String,
    pub protocol: String,
    #[serde(deserialize_with = "lit_none_string")]
    pub acodec: Option<String>,
    #[serde(deserialize_with = "lit_none_string")]
    pub vcodec: Option<String>,
    pub url: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
    pub rows: Option<i64>,
    pub columns: Option<i64>,
    pub fragments: Option<Vec<Fragment>>,
    #[serde(deserialize_with = "lit_none_string")]
    pub resolution: Option<String>,
    pub aspect_ratio: Option<f64>,
    // pub http_headers: Option<HashMap<String, String>>,
    pub audio_ext: String,
    pub video_ext: String,
    pub format: String,
    pub asr: Option<i64>,
    pub filesize: Option<u64>,
    pub source_preference: Option<i64>,
    pub audio_channels: Option<i64>,
    pub quality: Option<f64>,
    pub has_drm: Option<bool>,
    pub tbr: Option<f64>,
    pub language_preference: Option<i64>,
    pub abr: Option<f64>,
    pub container: Option<String>,
    pub preference: Option<i64>,
    pub dynamic_range: Option<String>,
    pub vbr: Option<f64>,
    pub filesize_approx: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Fragment {
    pub url: Option<String>,
    pub path: Option<String>,
    pub duration: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Thumbnail {
    pub url: String,
    pub preference: Option<i64>,
    pub id: String,
    pub height: Option<i64>,
    pub width: Option<i64>,
    pub resolution: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
    pub release_git_head: String,
    pub repository: String,
}

fn lit_none_string<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = std::convert::Infallible>,
    D: Deserializer<'de>,
{
    struct LitNoneString<T>(PhantomData<fn() -> T>);

    impl<'de, T> serde::de::Visitor<'de> for LitNoneString<T>
    where
        T: Deserialize<'de> + FromStr<Err = core::convert::Infallible>,
    {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match v {
                "none" => Ok(None),
                _ => Ok(Some(FromStr::from_str(v).unwrap())),
            }
        }
    }

    deserializer.deserialize_any(LitNoneString(PhantomData))
}

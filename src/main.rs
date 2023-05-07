use std::{cmp::Reverse, fmt::Display, fs::File, io::BufReader, process::Command};

use anyhow::{bail, Context};
use clap::Parser;
use humansize::{SizeFormatter, BINARY};
use inquire::Select;
use tempfile::TempDir;

mod infojson;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Url of the media to download
    url: String,

    /// Extra arguments to pass to yt-dlp
    #[arg(last = true)]
    extras: Vec<String>,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let tempdir = std::mem::ManuallyDrop::new(
        TempDir::new().context("couldn't create the temporary directory")?,
    );

    let mut command = Command::new("yt-dlp");

    command
        .arg("--write-info-json")
        .arg("--skip-download")
        .arg("--no-playlist")
        .arg("-P")
        .arg(tempdir.path())
        .arg(&args.url)
        .args(&args.extras);

    let status = command.status()?;

    if !status.success() {
        bail!("yt-dlp error: {:?}", command);
    }

    let info_json_entry = std::fs::read_dir(tempdir.path())
        .with_context(|| tempdir.path().display().to_string())?
        .find_map(|entry| {
            if let Ok(entry) = entry {
                if entry.file_type().ok().map_or(false, |ft| ft.is_file()) {
                    Some(entry)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .context("directory empty")?;

    let info_json =
        BufReader::new(File::open(info_json_entry.path()).with_context(|| {
            format!("unable to open file: {}", info_json_entry.path().display())
        })?);
    let info_json: infojson::InfoJson = serde_json::from_reader(info_json).with_context(|| {
        format!(
            "unable to read the info_json file: {}",
            info_json_entry.path().display()
        )
    })?;

    let mut formats = Vec::new();

    let is_music = info_json.categories.as_ref().map_or(false, |categories| {
        categories
            .iter()
            .any(|cat| cat.eq_ignore_ascii_case("music"))
    });

    if is_music {
        match prep_select_audio(info_json.formats.iter()).prompt() {
            Ok(AudioFormatDisplay(format)) => formats.push(&format.format_id),
            Err(_) => return Ok(()),
        }
        match prep_select_video(info_json.formats.iter()).prompt_skippable() {
            Ok(Some(VideoFormatDisplay(format))) => formats.push(&format.format_id),
            Ok(None) => {}
            Err(_) => return Ok(()),
        }
    } else {
        match prep_select_video(info_json.formats.iter()).prompt() {
            Ok(VideoFormatDisplay(format)) => formats.push(&format.format_id),
            Err(_) => return Ok(()),
        }
        match prep_select_audio(info_json.formats.iter()).prompt() {
            Ok(AudioFormatDisplay(format)) => formats.push(&format.format_id),
            Err(_) => return Ok(()),
        }
    }

    let mut command = Command::new("yt-dlp");

    command
        .arg("--load-info-json")
        .arg(info_json_entry.path())
        .arg("--no-playlist")
        .arg("-f")
        .arg({
            let mut ff = String::new();

            ff.push_str(&formats[0]);
            for f in &formats[1..] {
                ff.push_str("+");
                ff.push_str(f);
            }

            ff
        })
        .args(&args.extras);

    let status = command.status()?;

    if !status.success() {
        bail!("yt-dlp error: {:?}", command);
    }

    drop(tempdir);
    Ok(())
}
struct AudioFormatDisplay<'a>(&'a infojson::Format);

impl Display for AudioFormatDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(acodec) = &self.0.acodec {
            write!(f, "{:4.4}", acodec)?;
        }
        if let Some(asr) = self.0.asr {
            f.write_str(" ")?; // todo
            write!(f, "{}k", asr / 1000)?;
        }
        if let Some(filesize) = self.0.filesize {
            f.write_str(" ")?; // todo
            write!(f, "{}", SizeFormatter::new(filesize, BINARY))?;
        }
        if let Some(format_note) = &self.0.format_note {
            f.write_str(" ")?; // todo
            f.write_str(&format_note)?;
        }
        Ok(())
    }
}

fn prep_select_audio<'a, I: Iterator<Item = &'a infojson::Format>>(
    formats: I,
) -> Select<'a, AudioFormatDisplay<'a>> {
    let mut options: Vec<AudioFormatDisplay> = formats
        .filter(|f| f.acodec.is_some() && f.vcodec.is_none())
        .map(AudioFormatDisplay)
        .collect();

    options.sort_unstable_by_key(|f| Reverse(&f.0.asr));

    Select::new("Which audio format do you want?", options).with_formatter(&|f| {
        let mut buf = String::new();

        buf.push_str(&f.value.0.format_id);
        if let Some(acodec) = &f.value.0.acodec {
            buf.push_str(" - ");
            buf.push_str(acodec);
        }

        buf
    })
}

struct VideoFormatDisplay<'a>(&'a infojson::Format);

impl Display for VideoFormatDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(vcodec) = &self.0.vcodec {
            write!(f, "{:4.4}", vcodec)?;
        }
        if let Some(resolution) = &self.0.resolution {
            write!(f, " {}", resolution)?;
        }
        if let Some(filesize) = self.0.filesize {
            f.write_str(" ")?; // todo
            write!(f, "{}", SizeFormatter::new(filesize, BINARY))?;
        }
        if let Some(format_note) = &self.0.format_note {
            f.write_str(" ")?; // todo
            f.write_str(&format_note)?;
        }
        Ok(())
    }
}

fn prep_select_video<'a, I: Iterator<Item = &'a infojson::Format>>(
    formats: I,
) -> Select<'a, VideoFormatDisplay<'a>> {
    let mut options: Vec<VideoFormatDisplay> = formats
        .filter(|f| f.vcodec.is_some() && f.acodec.is_none())
        .map(VideoFormatDisplay)
        .collect();

    options.sort_unstable_by_key(|f| Reverse(&f.0.width));

    Select::new("Which video format do you want?", options).with_formatter(&|f| {
        let mut buf = String::new();

        buf.push_str(&f.value.0.format_id);
        if let Some(vcodec) = &f.value.0.vcodec {
            buf.push_str(" - ");
            buf.push_str(vcodec);
        }

        buf
    })
}

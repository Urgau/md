use std::{
    borrow::Cow, cmp::Reverse, fmt::Display, fs::File, io::BufReader, path::Path, process::Command,
};

use anyhow::{bail, Context};
use clap::{Parser, ValueEnum};
use humansize::{SizeFormatter, BINARY};
use inquire::{Confirm, Select, Text};
use tempfile::TempDir;

mod infojson;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Make yt-dlp output quiet
    #[arg(long)]
    quiet: bool,

    /// Preset to use
    #[arg(short, long, value_enum)]
    preset: Option<Preset>,

    /// Use XDG-dirs (~/Music or ~/Movie)
    #[arg(short, long)]
    dirs: bool,

    /// Url of the media to download
    url: String,

    /// Extra arguments to pass to yt-dlp
    #[arg(last = true)]
    extras: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Preset {
    /// Select custom format
    Custom,
    /// Define the format to use
    #[value(skip)]
    Manual,
    /// Use the "best" format
    Best,
    /// Best audio-only format
    BestAudio,
    /// Best video-only format
    BestVideo,
}

fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let tempdir = std::mem::ManuallyDrop::new(
        TempDir::new().context("couldn't create the temporary directory")?,
    );

    let mut command = Command::new("yt-dlp");

    if args.quiet {
        command.arg("--quiet");
    }

    command
        .arg("--write-info-json")
        .arg("--skip-download")
        .arg("--no-playlist")
        .arg("-P")
        .arg(tempdir.path())
        .arg(&args.url)
        .args(&args.extras);

    if args.verbose > 0 {
        println!(" -> executing: {:?}", command);
    }

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

    let mut formats: Vec<Cow<str>> = Vec::new();

    let is_music = info_json.categories.as_ref().map_or(false, |categories| {
        categories
            .iter()
            .any(|cat| cat.eq_ignore_ascii_case("music"))
    });

    let preset = if let Some(preset) = args.preset {
        preset
    } else {
        let presets = if is_music {
            &[
                Preset::BestAudio,
                Preset::Custom,
                Preset::Best,
                Preset::BestVideo,
                Preset::Manual,
            ]
        } else {
            &[
                Preset::Custom,
                Preset::Best,
                Preset::BestAudio,
                Preset::BestVideo,
                Preset::Manual,
            ]
        };

        match prep_select_preset(presets.iter().copied()).prompt() {
            Ok(PresetDisplay(preset)) => preset,
            Err(_) => return Ok(()),
        }
    };

    match preset {
        Preset::Custom => {
            match prep_select_video(info_json.formats.iter()).prompt() {
                Ok(VideoFormatDisplay(format)) => formats.push((&format.format_id).into()),
                Err(_) => return Ok(()),
            }
            match prep_select_audio(info_json.formats.iter()).prompt() {
                Ok(AudioFormatDisplay(format)) => formats.push((&format.format_id).into()),
                Err(_) => return Ok(()),
            }
        }
        Preset::BestAudio => formats.push("bestaudio".into()),
        Preset::BestVideo => formats.push("bestvideo".into()),
        Preset::Best => formats.push("bv*+ba/b".into()),
        Preset::Manual => match Text::new("Format?").prompt() {
            Ok(format) => formats.push(format.into()),
            Err(_) => return Ok(()),
        },
    }

    let output_template = {
        let title = match Text::new("Title?")
            .with_initial_value(&info_json.title)
            .prompt()
        {
            Ok(title) => title,
            Err(_) => return Ok(()),
        };

        format!("{title}.%(ext)s")
    };

    let embed_thumbnail = {
        match Confirm::new("Embed thumbnail?")
            .with_default(
                matches!(preset, Preset::BestAudio | Preset::BestVideo)
                    && matches!(Path::new("/bin/mutagen-inspect").try_exists(), Ok(true)),
            )
            .prompt()
        {
            Ok(confirm) => confirm,
            Err(_) => return Ok(()),
        }
    };

    let embed_chapters = if !matches!(preset, Preset::BestAudio) {
        match Confirm::new("Embed chapters?")
            .with_default(matches!(preset, Preset::Best | Preset::BestVideo))
            .prompt()
        {
            Ok(confirm) => confirm,
            Err(_) => return Ok(()),
        }
    } else {
        false
    };

    let mut command = Command::new("yt-dlp");

    if args.quiet {
        command.arg("--quiet");
    }

    if args.dirs {
        let output = if matches!(preset, Preset::BestAudio) {
            dirs::audio_dir().context("cloudn't get the audio directory")?
        } else {
            dirs::video_dir().context("couldn't get the video directory")?
        };

        command.arg("-P").arg(output);
    }

    if matches!(preset, Preset::BestAudio) {
        command.arg("-x");
    }

    if embed_thumbnail {
        command.arg("--embed-thumbnail");
    } else {
        command.arg("--no-embed-thumbnail");
    }

    if embed_chapters {
        command.arg("--embed-chapters");
    } else {
        command.arg("--no-embed-chapters");
    }

    command
        .arg("--load-info-json")
        .arg(info_json_entry.path())
        .arg("--no-playlist")
        .arg("-o")
        .arg(output_template)
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

    if args.verbose > 0 {
        println!(" -> executing: {:?}", command);
    }

    let status = command.status()?;

    if !status.success() {
        bail!("yt-dlp error: {:?}", command);
    }

    drop(std::mem::ManuallyDrop::into_inner(tempdir));
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

struct PresetDisplay(Preset);

impl Display for PresetDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Preset::Custom => write!(f, "custom"),
            Preset::Manual => write!(f, "manual"),
            Preset::Best => write!(f, "best"),
            Preset::BestAudio => write!(f, "best audio"),
            Preset::BestVideo => write!(f, "best video"),
        }
    }
}

fn prep_select_preset<'a, I: Iterator<Item = Preset>>(presets: I) -> Select<'a, PresetDisplay> {
    let presets = presets.map(PresetDisplay).collect();
    Select::new("Which preset do you want to use?", presets)
}

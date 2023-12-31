use std::{borrow::Cow, cmp::Reverse, fmt::Display, fs::File};
use std::{io::BufReader, path::Path, process::Command};

use anyhow::{bail, Context};
use clap::{Parser, ValueEnum};
use humansize::{SizeFormatter, BINARY};
use inquire::{Confirm, MultiSelect, Select, Text};
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
    /// Manual format to use
    #[value(skip)]
    Manual,
    /// Select a custom format
    Custom,
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

    let has_some_video_only_format = info_json
        .formats
        .iter()
        .any(|f| f.vcodec.is_some() && f.acodec.is_none());
    let has_some_audio_only_format = info_json
        .formats
        .iter()
        .any(|f| f.vcodec.is_none() && f.acodec.is_some());

    let preset = if let Some(preset) = args.preset {
        preset
    } else {
        let presets = if has_some_audio_only_format && has_some_video_only_format {
            &[
                Preset::Manual,
                Preset::Custom,
                Preset::Best,
                Preset::BestAudio,
                Preset::BestVideo,
            ] as &[_]
        } else if has_some_audio_only_format {
            &[
                Preset::Manual,
                Preset::Custom,
                Preset::Best,
                Preset::BestAudio,
            ] as &[_]
        } else if has_some_video_only_format {
            &[
                Preset::Manual,
                Preset::Custom,
                Preset::Best,
                Preset::BestVideo,
            ] as &[_]
        } else {
            &[Preset::Manual, Preset::Custom, Preset::Best] as &[_]
        };

        match prep_select_preset(presets.iter().copied())
            .with_starting_cursor(if is_music { 3 } else { 2 })
            .prompt()
        {
            Ok(PresetDisplay(preset)) => preset,
            Err(_) => return Ok(()),
        }
    };

    match preset {
        Preset::Custom => {
            let video_format = match prep_select_video(info_json.formats.iter()).prompt() {
                Ok(VideoFormatDisplay(format)) => format,
                Err(_) => return Ok(()),
            };
            formats.push((&video_format.format_id).into());
            if video_format.acodec.is_none() {
                match prep_select_audio(info_json.formats.iter()).prompt() {
                    Ok(AudioFormatDisplay(format)) => formats.push((&format.format_id).into()),
                    Err(_) => return Ok(()),
                }
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

    let embed_subtitles = if let Some(subtitles) = &info_json.subtitles {
        if !matches!(preset, Preset::BestAudio) && !subtitles.is_empty() {
            let subs = subtitles.iter().flat_map(|(n, s)| match s {
                infojson::Subtitles::Normal(s) => Some((n.as_ref(), s.as_slice())),
                _ => None,
            });
            match prep_multiselect_subtitle(subs).prompt() {
                Ok(subs) if !subs.is_empty() => Some(subs),
                Ok(_) => None,
                Err(_) => return Ok(()),
            }
        } else {
            None
        }
    } else {
        None
    };

    let sponsorblock_remove = if info_json.extractor_key.eq_ignore_ascii_case("youtube")
        && !matches!(preset, Preset::BestAudio)
    {
        match Confirm::new("Remove sponsor blocks?")
            .with_default(false)
            .with_help_message("warn: will reencode")
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

    if sponsorblock_remove {
        command.arg("--sponsorblock-remove=default");
    } else {
        command.arg("--no-sponsorblock");
    }

    if let Some(embed_subs) = embed_subtitles {
        command.arg("--embed-subs");
        for sublang in embed_subs {
            command.arg("--sub-lang");
            command.arg(sublang.0);
        }
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
        if let Some(protocol) = Some(&self.0.protocol) {
            f.write_str(" (")?; // todo
            f.write_str(&protocol)?;
            f.write_str(")")?; // todo
        }
        Ok(())
    }
}

fn prep_select_audio<'a, I: Iterator<Item = &'a infojson::Format>>(
    formats: I,
) -> Select<'a, AudioFormatDisplay<'a>> {
    let mut options: Vec<AudioFormatDisplay> = formats
        .filter(|f| f.acodec.is_some() /*&& f.vcodec.is_none()*/)
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
        if let Some(protocol) = Some(&self.0.protocol) {
            f.write_str(" (")?; // todo
            f.write_str(&protocol)?;
            f.write_str(")")?; // todo
        }
        Ok(())
    }
}

fn prep_select_video<'a, I: Iterator<Item = &'a infojson::Format>>(
    formats: I,
) -> Select<'a, VideoFormatDisplay<'a>> {
    let mut options: Vec<VideoFormatDisplay> = formats
        .filter(|f| f.vcodec.is_some() /*&& f.acodec.is_none()*/)
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

struct SubtitleDisplay<'a>(&'a str, &'a [infojson::SubtitleInfo]);

impl Display for SubtitleDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.1
                .get(0)
                .map(|info| info.name.as_deref())
                .flatten()
                .unwrap_or(self.0)
        )
    }
}

fn prep_multiselect_subtitle<'a, I: Iterator<Item = (&'a str, &'a [infojson::SubtitleInfo])>>(
    subs: I,
) -> MultiSelect<'a, SubtitleDisplay<'a>> {
    let subs = subs.map(|(a, b)| SubtitleDisplay(a, b)).collect();
    MultiSelect::new("Do you want to embed a subtitle?", subs)
}

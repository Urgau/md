# md

Terminal (console) guided yt-dlp

## Example

```shell
$ md "https://www.youtube.com/watch?v=QfFG_UJYkFo&pp=ygUgVG9idSAtIENhbmR5bGFuZCAoS2FqYWNrcyBSZW1peCk%3D"
[...]
[youtube] QfFG_UJYkFo: Downloading m3u8 information
[info] QfFG_UJYkFo: Downloading 1 format(s): 137+251
> Which preset do you want to use? best
> Title? Tobu Candyland - Remix
> Embed thumbnail? No
> Embed chapters? Yes
> Do you want to embed a subtitle? en
[...]
[info] QfFG_UJYkFo: Downloading subtitles: en
[info] QfFG_UJYkFo: Downloading 1 format(s): hls-2572
[...]
```

## Options

```
$ md --help
Usage: md [OPTIONS] <URL> [-- <EXTRAS>...]

Arguments:
  <URL>
          Url of the media to download

  [EXTRAS]...
          Extra arguments to pass to yt-dlp

Options:
  -v, --verbose...
          Verbosity

      --quiet
          Make yt-dlp output quiet

  -p, --preset <PRESET>
          Preset to use

          Possible values:
          - custom:     Select a custom format
          - best:       Use the "best" format
          - best-audio: Best audio-only format
          - best-video: Best video-only format

  -d, --dirs
          Use XDG-dirs (~/Music or ~/Movie)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

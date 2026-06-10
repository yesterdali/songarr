#!/usr/bin/env bash
# Generate the seeded test library: 20 sine-wave tracks (mp3 + flac) across
# 4 artists / 4 albums, tagged via ffmpeg metadata, plus folder cover art.
set -euo pipefail
cd "$(dirname "$0")"

MUSIC_DIR="data/music"

if ! command -v ffmpeg >/dev/null; then
    echo "ffmpeg is required on PATH to seed test audio" >&2
    exit 1
fi

# Fixture source for the mock yt-dlp: ~3 minutes of opus in webm (≈2 MB),
# big enough that the disconnect test crosses the keep-download threshold.
mkdir -p data/fixtures
if [ ! -f data/fixtures/source.webm ]; then
    ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=440:duration=180" \
        -c:a libopus -b:a 96k data/fixtures/source.webm
    echo "seed: generated data/fixtures/source.webm"
fi

if [ -f "$MUSIC_DIR/.seeded" ]; then
    echo "seed: $MUSIC_DIR already seeded, skipping (rm -rf $MUSIC_DIR to regenerate)"
    exit 0
fi

mkdir -p "$MUSIC_DIR"

# make_track artist album year ext track_no title freq
make_track() {
    local artist="$1" album="$2" year="$3" ext="$4" track="$5" title="$6" freq="$7"
    local dir="$MUSIC_DIR/$artist/$album"
    local file
    file="$dir/$(printf '%02d' "$track") - $title.$ext"
    mkdir -p "$dir"
    local codec
    case "$ext" in
        mp3) codec="-c:a libmp3lame -b:a 128k" ;;
        flac) codec="-c:a flac" ;;
    esac
    # shellcheck disable=SC2086
    ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=$freq:duration=4" \
        -ar 44100 $codec \
        -metadata artist="$artist" -metadata album="$album" -metadata title="$title" \
        -metadata track="$track" -metadata date="$year" -metadata genre="Test Tone" \
        "$file"
}

# make_cover artist album hexcolor
make_cover() {
    local dir="$MUSIC_DIR/$1/$2"
    mkdir -p "$dir"
    ffmpeg -loglevel error -y -f lavfi -i "color=c=0x$3:size=600x600,format=rgb24" \
        -frames:v 1 "$dir/cover.jpg"
}

artist="The Sine Waves";    album="Pure Tones";  year=2021
make_cover "$artist" "$album" 336699
for i in 1 2 3 4 5 6; do
    make_track "$artist" "$album" "$year" mp3 "$i" "Tone $((200 + i * 20)) Hz" "$((200 + i * 20))"
done

artist="Square Pulse";      album="Duty Cycle";  year=2022
make_cover "$artist" "$album" 993333
for i in 1 2 3 4 5; do
    make_track "$artist" "$album" "$year" flac "$i" "Pulse $((300 + i * 25)) Hz" "$((300 + i * 25))"
done

artist="Sawtooth Sisters";  album="Ramp Up";     year=2023
make_cover "$artist" "$album" 339966
for i in 1 2 3 4 5; do
    make_track "$artist" "$album" "$year" mp3 "$i" "Ramp $((440 + i * 15)) Hz" "$((440 + i * 15))"
done

artist="Noise Floor";       album="Pink";        year=2024
make_cover "$artist" "$album" 996699
for i in 1 2 3 4; do
    make_track "$artist" "$album" "$year" flac "$i" "Floor $((520 + i * 10)) Hz" "$((520 + i * 10))"
done

touch "$MUSIC_DIR/.seeded"
echo "seed: generated $(find "$MUSIC_DIR" -name '*.mp3' -o -name '*.flac' | wc -l | tr -d ' ') tracks in $MUSIC_DIR"

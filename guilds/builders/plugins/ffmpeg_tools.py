import json
import logging
import os
import tempfile
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-ffmpeg-tools")

FFMPEG = os.environ.get("FFMPEG_PATH") or "ffmpeg"
FFPROBE = os.environ.get("FFPROBE_PATH") or "ffprobe"


def _auto_output(input_path: str, suffix: str, ext: str | None = None) -> str:
    p = Path(input_path)
    out_ext = ext or p.suffix
    return str(p.parent / f"{p.stem}_{suffix}{out_ext}")


async def _run_ffmpeg(cmd: list[str], timeout: int = 120) -> tuple[int, str, str]:
    return await utils.run_command(cmd, timeout_secs=timeout)


@mcp.tool()
async def media_probe(file_path: str) -> str:
    """Get technical info about a media file (duration, resolution, codec, bitrate)."""
    fp = Path(file_path).resolve()
    if not fp.exists():
        return f"File not found: {file_path}"
    rc, out, err = await _run_ffmpeg([FFPROBE, "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", str(fp)], timeout=15)
    if rc != 0:
        return f"Probe failed: {err}"
    try:
        info = json.loads(out)
    except json.JSONDecodeError:
        return f"Invalid ffprobe output: {out[:500]}"

    fmt = info.get("format", {})
    streams = info.get("streams", [])
    video = next((s for s in streams if s.get("codec_type") == "video"), None)
    audio = next((s for s in streams if s.get("codec_type") == "audio"), None)
    lines = [f"Media Info: {fp.name}"]
    lines.append(f"  Duration: {float(fmt.get('duration', 0)):.2f}s")
    size = int(fmt.get("size", 0)) / (1024 * 1024)
    lines.append(f"  Size: {size:.2f}MB")
    lines.append(f"  Format: {fmt.get('format_long_name') or fmt.get('format_name', 'unknown')}")
    if video:
        fps = None
        if video.get("r_frame_rate"):
            parts = video["r_frame_rate"].split("/")
            if len(parts) == 2 and int(parts[1]) > 0:
                fps = round(int(parts[0]) / int(parts[1]))
        lines.append(f"  Video: {video['codec_name']} {video.get('width', '?')}x{video.get('height', '?')}" + (f" @ {fps}fps" if fps else ""))
    if audio:
        lines.append(f"  Audio: {audio['codec_name']} {audio.get('sample_rate', '?')}Hz {audio.get('channels', '?')}ch")
    return "\n".join(lines)


@mcp.tool()
async def video_trim(file_path: str, start: str, end: str, output: str = "") -> str:
    """Trim a video to a specific time range. Creates a new file."""
    fp = Path(file_path).resolve()
    out = output or _auto_output(str(fp), "trimmed")
    rc, _, err = await _run_ffmpeg([FFMPEG, "-y", "-i", str(fp), "-ss", start, "-to", end, "-c", "copy", out])
    if rc != 0:
        return f"Trim failed: {err}"
    return f"Video trimmed: {start} -> {end}\n  Output: {out}"


@mcp.tool()
async def video_concat(files: list[str], output: str = "") -> str:
    """Concatenate multiple video files into one."""
    resolved = [str(Path(f).resolve()) for f in files]
    out = output or _auto_output(resolved[0], "concat")
    with tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False) as f:
        for fp in resolved:
            escaped = fp.replace("'", "'\\''")
            f.write(f"file '{escaped}'\n")
        list_path = f.name
    try:
        rc, _, err = await _run_ffmpeg([FFMPEG, "-y", "-f", "concat", "-safe", "0", "-i", list_path, "-c", "copy", out])
        if rc != 0:
            return f"Concat failed: {err}"
        return f"Concatenated {len(files)} files\n  Output: {out}"
    finally:
        Path(list_path).unlink(missing_ok=True)


@mcp.tool()
async def video_resize(file_path: str, width: int = -1, height: int = -1, output: str = "") -> str:
    """Resize a video to a different resolution."""
    fp = Path(file_path).resolve()
    w = width if width != -1 else -1
    h = height if height != -1 else -1
    out = output or _auto_output(str(fp), f"{w}x{h}")
    scale = f"scale={w}:{h}:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2"
    rc, _, err = await _run_ffmpeg([FFMPEG, "-y", "-i", str(fp), "-vf", scale, out])
    if rc != 0:
        return f"Resize failed: {err}"
    return f"Video resized to {w}x{h}\n  Output: {out}"


@mcp.tool()
async def audio_extract(file_path: str, format: str = "mp3", output: str = "") -> str:
    """Extract audio track from a video file as MP3/WAV/AAC."""
    fp = Path(file_path).resolve()
    out = output or _auto_output(str(fp), "audio", f".{format}")
    codec_map = {"mp3": "libmp3lame", "wav": "pcm_s16le", "aac": "aac"}
    codec = codec_map.get(format, "libmp3lame")
    rc, _, err = await _run_ffmpeg([FFMPEG, "-y", "-i", str(fp), "-vn", "-acodec", codec, out])
    if rc != 0:
        return f"Audio extraction failed: {err}"
    return f"Audio extracted ({format})\n  Output: {out}"


@mcp.tool()
async def video_add_text(file_path: str, text: str, position: str = "bottom", font_size: int = 24, output: str = "") -> str:
    """Add text overlay (subtitle/watermark) to a video."""
    fp = Path(file_path).resolve()
    out = output or _auto_output(str(fp), "text")
    y_positions = {"top": "30", "center": "(h-text_h)/2", "bottom": "h-text_h-30"}
    y_pos = y_positions.get(position, "h-text_h-30")
    escaped_text = text.replace("'", "'\\''").replace(":", "\\:")
    drawtext = f"drawtext=text='{escaped_text}':fontsize={font_size}:fontcolor=white:borderw=2:bordercolor=black:x=(w-text_w)/2:y={y_pos}"
    rc, _, err = await _run_ffmpeg([FFMPEG, "-y", "-i", str(fp), "-vf", drawtext, out])
    if rc != 0:
        return f"Text overlay failed: {err}"
    return f'Text overlay added: "{text}"\n  Position: {position}\n  Output: {out}'


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)

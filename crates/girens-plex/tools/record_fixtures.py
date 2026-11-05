#!/usr/bin/env python3
"""Record anonymised JSON fixtures from a Plex Media Server.

    PLEX_URL=https://plex.example.org PLEX_TOKEN=... \
        python3 crates/girens-plex/tools/record_fixtures.py

Only GET requests are sent. Responses are written to tests/fixtures/ after
anonymising: every string is replaced by a stable fake unless its field is
known to be harmless (types, codecs, API paths, ...), identifiers of the
server and account are replaced, durations and file sizes are rounded and
lists are cut short. Technical properties (codecs, resolutions, timestamps,
year, content rating) are kept. Review `git diff` before committing new recordings.
"""

import hashlib
import json
import os
import pathlib
import re
import sys
import urllib.parse
import urllib.request

FIXTURES = pathlib.Path(__file__).resolve().parent.parent / "tests" / "fixtures"
MAX_ITEMS = 3
MAX_TAGS = 2

# String fields that describe the format of the data rather than its content.
SAFE_STRING_KEYS = {
    "type", "subtype", "viewGroup", "viewMode", "identifier", "mediaTagPrefix",
    "mediaTagVersion", "playlistType", "scanner", "agent", "language",
    "languageCode", "languageTag", "container", "videoCodec", "audioCodec",
    "codec", "videoResolution", "videoProfile", "audioProfile", "profile",
    "protocol", "pixelFormat", "chromaSubsampling", "chromaLocation",
    "colorPrimaries", "colorRange", "colorSpace", "colorTrc", "frameRate",
    "videoFrameRate", "audioChannelLayout", "scanType", "streamIdentifier",
    "defaultDirection", "descKey", "firstCharacterKey", "contentRating",
    "apiVersion", "version", "platform", "decision",
    "location", "state", "thumbBlurHash", "artBlurHash", "displayTitle",
    "extendedDisplayTitle", "selectedSubtitle", "bitDepth", "level", "refFrames",
    "samplingRate", "has64bitOffsets", "optimizedForStreaming", "format",
    "defaultSortKey", "activeDirection", "default", "content", "directory",
    "filters", "refreshing", "allowSync", "hidden", "smart", "composite",
    "transcoderVideoResolutions", "transcoderVideoBitrates",
    "transcoderVideoQualities", "livetv", "musicAnalysis",
}
# Sort keys (`/library/sections/N/sorts`) use `key` and `title` for sort names.
SORT_KEY_PATTERN = re.compile(r"^[A-Za-z.]+(:desc)?$")
API_PATH_PATTERN = re.compile(r"^/(library|playlists|playQueues|:|hubs|photo|video|music|status)(/|$)")
DATE_KEYS = {"originallyAvailableAt"}
# Free-text fields: never kept, even when the text happens to be a number.
TEXT_KEYS = {
    "title", "titleSort", "originalTitle", "parentTitle", "grandparentTitle",
    "summary", "tagline", "tag", "studio", "file", "friendlyName",
}
ROUND_TO_MINUTE_KEYS = {"duration"}
TAG_LIST_KEYS = {
    "Genre", "Role", "Director", "Writer", "Country", "Collection", "Label",
    "Mood", "Style", "Similar", "Producer", "Guid", "Rating", "Image",
    "UltraBlurColors", "Field", "Location", "Chapter", "Marker", "Extras",
    "Review", "Preferences", "Related",
}
ITEM_LIST_KEYS = {"Metadata", "Hub"}


def fake(key: str, value: str) -> str:
    digest = hashlib.sha256(value.encode()).hexdigest()[:8]
    return f"{key}-{digest}"


def anonymise(node, key=None, inside_sorts=False):
    if isinstance(node, dict):
        cleaned = {
            child_key: anonymise(child, child_key, inside_sorts)
            for child_key, child in node.items()
        }
        if "file" in cleaned and isinstance(cleaned.get("size"), int):
            # Exact file sizes can identify a file; megabytes are enough.
            cleaned["size"] = (cleaned["size"] // 1_000_000) * 1_000_000
        return cleaned
    if isinstance(node, list):
        limit = MAX_TAGS if key in TAG_LIST_KEYS else MAX_ITEMS if key in ITEM_LIST_KEYS else None
        items = node if limit is None else node[:limit]
        return [anonymise(item, key, inside_sorts) for item in items]
    if isinstance(node, bool) or node is None:
        return node
    if isinstance(node, (int, float)):
        if key in ROUND_TO_MINUTE_KEYS:
            return int(round(node / 60000.0)) * 60000
        return node
    if isinstance(node, str):
        if key in SAFE_STRING_KEYS:
            return node
        if key in DATE_KEYS:
            return "2020-01-02"
        if node.isdigit() and key not in TEXT_KEYS:
            # Numeric identifiers (ratingKey, parentRatingKey, ...).
            return node
        if inside_sorts and key in ("key", "title") and SORT_KEY_PATTERN.match(node.replace(" ", "")):
            return node
        if API_PATH_PATTERN.match(node):
            # Keep the path shape, drop file names and query values.
            path, _, query = node.partition("?")
            path = re.sub(r"/file\.[A-Za-z0-9]+$", "/file.bin", path)
            if query:
                pairs = urllib.parse.parse_qsl(query, keep_blank_values=True)
                query = "&".join(
                    f"{name}={value if value.isdigit() else 'x'}" for name, value in pairs
                )
                return f"{path}?{query}"
            return path
        return fake(key or "value", node)
    return node


def fetch(base: str, token: str, path: str):
    request = urllib.request.Request(
        base.rstrip("/") + path,
        headers={
            "Accept": "application/json",
            "X-Plex-Token": token,
            "X-Plex-Client-Identifier": "girens-fixture-recorder",
            "X-Plex-Product": "Girens",
        },
        method="GET",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def main() -> int:
    base = os.environ.get("PLEX_URL")
    token = os.environ.get("PLEX_TOKEN")
    if not base or not token:
        print("Set PLEX_URL and PLEX_TOKEN.", file=sys.stderr)
        return 2

    FIXTURES.mkdir(parents=True, exist_ok=True)
    page = f"X-Plex-Container-Start=0&X-Plex-Container-Size={MAX_ITEMS}"

    def record(name: str, path: str, inside_sorts=False):
        try:
            data = fetch(base, token, path)
        except Exception as error:  # noqa: BLE001 - report and continue
            print(f"  skipped {name}: {error}", file=sys.stderr)
            return None
        cleaned = anonymise(data, inside_sorts=inside_sorts)
        container = cleaned.get("MediaContainer")
        if isinstance(container, dict):
            for list_key in ITEM_LIST_KEYS:
                if list_key in container and "size" in container:
                    container["size"] = len(container[list_key])
        (FIXTURES / f"{name}.json").write_text(
            json.dumps(cleaned, indent=2, ensure_ascii=False, sort_keys=True) + "\n"
        )
        print(f"  recorded {name}")
        return data

    def first(data, *types):
        for item in (data or {}).get("MediaContainer", {}).get("Metadata", []):
            if not types or item.get("type") in types:
                return item
        return None

    record("server_root", "/")
    record("identity", "/identity")
    sections = record("library_sections", "/library/sections")
    record("recently_added", f"/library/recentlyAdded?{page}")
    record("on_deck", f"/library/onDeck?{page}")
    playlists = record("playlists", f"/playlists?{page}")
    playlist = first(playlists)
    if playlist:
        record("playlist_items", f"/playlists/{playlist['ratingKey']}/items?{page}")

    seen_types = set()
    for section in (sections or {}).get("MediaContainer", {}).get("Directory", []):
        kind, key = section.get("type"), section.get("key")
        if kind in seen_types or kind not in ("movie", "show", "artist"):
            continue
        seen_types.add(kind)
        items = record(f"section_{kind}_all", f"/library/sections/{key}/all?{page}&sort=titleSort:asc")
        record(f"section_{kind}_sorts", f"/library/sections/{key}/sorts", inside_sorts=True)
        record(f"section_{kind}_on_deck", f"/library/sections/{key}/onDeck?{page}")
        item = first(items)
        if not item:
            continue
        rating_key = item["ratingKey"]
        record(f"{kind}_metadata", f"/library/metadata/{rating_key}")
        search_type = {"movie": 1, "show": 2, "artist": 8}[kind]
        query = urllib.parse.quote(item.get("title", "a")[:3])
        record(f"search_{kind}", f"/library/all?title={query}&type={search_type}&{page}")

        if kind == "show":
            seasons = record("show_children", f"/library/metadata/{rating_key}/children?{page}")
            episodes = record("show_all_leaves", f"/library/metadata/{rating_key}/allLeaves?{page}")
            season = first(seasons, "season")
            if season:
                record("season_metadata", f"/library/metadata/{season['ratingKey']}")
            episode = first(episodes, "episode")
            if episode:
                record("episode_metadata", f"/library/metadata/{episode['ratingKey']}")
        elif kind == "artist":
            albums = record("artist_children", f"/library/metadata/{rating_key}/children?{page}")
            record("section_artist_albums", f"/library/sections/{key}/albums?{page}")
            album = first(albums, "album")
            if album:
                record("album_metadata", f"/library/metadata/{album['ratingKey']}")
                tracks = record("album_children", f"/library/metadata/{album['ratingKey']}/children?{page}")
                track = first(tracks, "track")
                if track:
                    record("track_metadata", f"/library/metadata/{track['ratingKey']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

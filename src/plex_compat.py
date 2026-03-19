"""Compatibility helpers that implement functionality from the tijder/python-plexapi fork
that is not present in the upstream pushingkarmaorg/python-plexapi package.
"""

import re
from urllib.parse import urlencode

from plexapi.media import AudioStream, SubtitleStream
from plexapi.playqueue import PlayQueue


def get_stream_url(item, **params):
    """Get a stream URL with support for direct play and transcoding with session.

    Extends the upstream getStreamURL to support the directPlay parameter
    and the fileExtension parameter used for DASH streams.
    """
    if params.pop('directPlay', False):
        locations = [i for i in item.iterParts() if i]
        for location in locations:
            return item._server.url(location.key, includeToken=True)

    params.pop('fileExtension', None)
    return item.getStreamURL(**params)


def get_decision(item, session, **params):
    """Request a transcode decision from the Plex server."""
    mvb = params.get('maxVideoBitrate')
    vr = params.get('videoResolution', '')
    api_params = {
        'hasMDE':            '1',
        'path':              item.key,
        'session':           session,
        'protocol':          params.get('protocol', 'hls'),
        'directPlay':        '0',
        'directStream':      '1',
        'fastSeek':          '1',
        'mediaIndex':        params.get('mediaIndex', 0),
        'partIndex':         params.get('partIndex', 0),
        'location':          params.get('location', 'lan'),
        'autoAdjustQuality': '0',
        'directStreamAudio': '1',
        'subtitles':         'burn',
        'subtitleSize':      100,
        'copyts':            '1',
        'maxVideoBitrate':   max(mvb, 64) if mvb else None,
        'videoResolution':   vr if re.match(r'^\d+x\d+$', vr) else None,
        'audioBoost':        100,
        'X-Plex-Platform':   params.get('platform', 'Chrome'),
    }
    api_params = {k: v for k, v in api_params.items() if v is not None}
    streamtype = 'audio' if item.TYPE in ('track', 'album') else 'video'
    sorted_params = sorted(api_params.items(), key=lambda val: val[0])
    return item._server.query('/%s/:/transcode/universal/decision?%s' % (streamtype, urlencode(sorted_params)))


def ping_session(item, session):
    """Keep a transcode session alive."""
    params = {'session': session}
    streamtype = 'audio' if item.TYPE in ('track', 'album') else 'video'
    return item._server.query('/%s/:/transcode/universal/ping?%s' % (streamtype, urlencode(params)))


def update_timeline(item, time, state='stopped', duration=None, playQueueItemID=None, session=None):
    """Update the playback timeline, including session and playQueueItemID parameters."""
    duration_val = duration if duration is not None else item.duration
    key = (
        f'/:/timeline?ratingKey={item.ratingKey}&key={item.key}'
        f'&identifier=com.plexapp.plugins.library&time={int(time)}&state={state}'
        f'&duration={duration_val}'
    )
    if playQueueItemID is not None:
        key += f'&playQueueItemID={playQueueItemID}'
    if session is not None:
        key += f'&X-Plex-Provider-Version=5.1&X-Plex-Session-Identifier={session}'
    item._server.query(key)
    item.reload()


def get_selected_audio_stream(item):
    """Return the currently selected audio stream."""
    for part in item.iterParts():
        for stream in part.audioStreams():
            if stream.selected is True:
                return stream
    return None


def get_selected_subtitle_stream(item):
    """Return the currently selected subtitle stream."""
    for part in item.iterParts():
        for stream in part.subtitleStreams():
            if stream.selected is True:
                return stream
    return None


def get_stream(item, stream_id):
    """Return a stream by ID."""
    for part in item.iterParts():
        for stream in part.streams:
            if stream.id == stream_id:
                return stream
    return None


def set_default_audio_stream(item, stream):
    """Set the default audio stream for playback."""
    stream_id = stream.id if isinstance(stream, AudioStream) else stream
    for part in item.iterParts():
        for s in part.audioStreams():
            if s.id == stream_id:
                part.setSelectedAudioStream(s)
                return


def set_default_subtitle_stream(item, stream):
    """Set the default subtitle stream for playback, or disable subtitles if stream is None."""
    if stream is None:
        for part in item.iterParts():
            part.resetSelectedSubtitleStream()
        return
    stream_id = stream.id if isinstance(stream, SubtitleStream) else stream
    for part in item.iterParts():
        for s in part.subtitleStreams():
            if s.id == stream_id:
                part.setSelectedSubtitleStream(s)
                return


def get_download_url(stream):
    """Return a download URL for a subtitle stream."""
    return stream._server.url('%s?download=1' % stream.key, includeToken=True)


def playqueue_from_url(server, path, key):
    """Create a PlayQueue from an existing URL path."""
    data = server.query(path, method=server._session.get)
    pq = PlayQueue(server, data, initpath=path)
    pq.key = key
    pq.repeat = 0
    pq.includeChapters = 1
    pq.includeRelated = 1
    return pq

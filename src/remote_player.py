from plex_mpv_shim.player_abstract import PlayerAbstract
from urllib.parse import urlsplit
from plexapi.server import PlexServer
from plexapi.playqueue import PlayQueue
from gi.repository import GLib
import threading


class RemotePlayer(PlayerAbstract):
    def __init__(self, player, window):
        self._player = player
        self._window = window
        self._player.connect('media-playing', self._on_current_song_changed)
        self._player.connect('media-paused', self._on_player_state_changed)
        self._player.connect("media-time", self.__on_media_time)

    def _on_current_song_changed(self, player, playing, playqueue_item, playqueue, offset, item):
        self.timeline_handle()

    def _on_player_state_changed(self, klass, args):
        self.timeline_handle()

    def __on_media_time(self, player, time):
        self.timeline_handle()

    def timeline_handle(self):
        if self.timeline_trigger:
            self.timeline_trigger.set()

    def get_state(self):
        return self._player.get_state().lower()

    def get_type(self):
        if self._player._item == None:
            return 'audio'
        elif self._player._item.listType == 'video':
            return 'video'
        elif self._player._item.listType == 'audio':
            return 'music'

    def is_playing(self):
        return (self._player.get_state().lower() == 'playing')

    def has_media_item(self):
        return self._player.has_media_item()

    def get_play_time(self):
        time = self._player.get_position()
        if time == None:
            return 0
        else:
            return round(time * 1000)

    def get_ratingKey(self):
        return self._player._item.ratingKey

    def get_key(self):
        return self._player._item.key

    def get_guid(self):
        return self._player._item.guid

    def get_duration(self):
        return self._player._item.duration

    def get_hostname(self):
        return self.__get_host_info().hostname

    def get_scheme(self):
        return self.__get_host_info().scheme

    def get_port(self):
        return self.__get_host_info().port

    def get_machine_identifier(self):
        return self._player._item._server.machineIdentifier

    def has_play_queue(self):
        return self._player._playqueue != None

    def get_playQueueID(self):
        return self._player._playqueue.playQueueID

    def get_playQueueVersion(self):
        return self._player._playqueue.playQueueVersion

    def get_playQueueItemID(self):
        return self._player._playqueue.playQueueSelectedItemID

    def get_playQueueKey(self):
        return self._player._playqueue.key

    def has_next(self):
        return self._player.has_next()

    def has_prev(self):
        return self._player.has_prev()

    def get_volume(self):
        return self._player.get_volume()

    def handle_play(self, address, protocol, port, key, offset, playQueue, token):
        tmp_server = PlexServer(protocol + "://" + address + ":" + port, token)
        playqueue = PlayQueue.get_from_url(tmp_server, playQueue, key)
        if playqueue.items[0].listType == 'video':
            GLib.idle_add(self._window.go_fullscreen)
        self._player.set_playqueue(playqueue)
        GLib.idle_add(self._player.start(offset_param=offset))
        #thread = threading.Thread(target=self._player.start,kwargs={'offset_param':offset,})
        #thread.daemon = True
        #thread.start()

    def stop(self):
        self._player.stop()

    def toggle_pause(self):
        self._player.play_pause()

    def play_next(self):
        self._player.next()

    def play_prev(self):
        self._player.prev()

    def step_forward(self):
        self._player.seek_forward()

    def step_back(self):
        self._player.seek_backward()

    def seek(self, seek):
        self._player.seek_to_time(seek)

    def skip_to(self, key):
        self._player.play_from_key(key)

    def set_volume(self, percent):
        self._player.set_volume(percent)

    def update_play_queue(self):
        self._player.refresh_playqueue()

    def get_product_name(self):
        return "Girens"

    def __get_host_info(self):
        parsed = urlsplit(self._player._item._server._baseurl)
        return parsed

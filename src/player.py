import gi
import mpv
import threading
import requests

from urllib.parse import urlparse
from plexapi import BASE_HEADERS

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GdkPixbuf, GObject

# import gi
# gi.require_version('Gtk', '3.0')
# locale.setlocale(locale.LC_NUMERIC, 'C')
# player = mpv.MPV()

class Player(GObject.Object):
    __gsignals__ = {
        'media-paused': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'media-playing': (GObject.SignalFlags.RUN_FIRST, None, (bool,object, object, int)),
        'media-time': (GObject.SignalFlags.RUN_FIRST, None, (int,)),
    }

    __instance = None
    @staticmethod
    def getInstance():
        """ Static access method. """
        if Player.__instance == None:
            Player()
        return Player.__instance

    def __init__(self, **kwargs):
        """ Virtually private constructor. """
        if Player.__instance != None:
            raise Exception("This class is a singleton!")
        else:
            super().__init__(**kwargs)

            self._player = None
            self._progresUpdate = None
            self._progresNow = None
            self._item = None
            self._stop_command = False
            self._playing = False
            self._play_wait = False
            self._next_index = None
            self._fullscreen = False
            self._controller = None
            Player.__instance = self

    def set_plex(self, plex):
        self._plex = plex

    def set_controller(self, controller):
        self._controller = controller

    def set_commandID(self, commandID):
        self._commandID = commandID

    def unset_controller(self):
        self._controller = None


    def __createPlayer(self):
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        self._player = mpv.MPV(input_default_bindings=True, input_vo_keyboard=True, vo='x11', title=self._item.title, keep_open=True)

        @self._player.property_observer('time-pos')
        def __time_observer(_name, value):
            if value is not None:
                self.emit('media-time', value * 1000)
            else:
                self.emit('media-time', 0)
            if value is not None and abs(value - self._progresUpdate) > 5:
                self._progresUpdate = value
                self.__on_update(state='playing')
            pass

        @self._player.property_observer('pause')
        def __on_pause(_name, value):
            self.emit('media-paused', value)
            if (value == True):
                self.__on_update(state='paused')

        @self._player.property_observer('eof-reached')
        def __on_eof(_name, value):
            if (value == True):
                self._eof = True
                self._player.command('stop')

        @self._player.property_observer('fullscreen')
        def __on_fullscreen(_name, value):
            self._fullscreen=value

    def __stop(self):
        self._player.terminate()
        self.__on_update(state='stopped')
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        self.__createPlayer()

    def set_playqueue(self, playqueue):
        self._playqueue = playqueue
        self.__playqueue_refresh()

    def start(self, from_beginning=False):
        if (self._playing != False):
            self._play_wait = True
            self._player.command('stop')
        else:
            self._next = False
            self._prev = False
            self._eof = False
            self._playing = True
            self._stop_command = False
            self._item = self._playqueue.items[self._offset]
            self.__createPlayer()
            self._progresUpdate = 0
            self._progresNow = 0

            if (from_beginning == False):
                offset = self._item.viewOffset / 1000
            else:
                offset = 0

            source = self._plex.get_item_download_path(self._item)
            self._player.fullscreen = self._fullscreen
            if (source == None):
                source = self._item.getStreamURL(offset=offset)
                self._player.play(source)
            else:
                self._player.play(source)
                self._player.wait_for_property('seekable')
                self._player.seek(offset, reference='absolute', precision='exact')
            self.emit('media-playing', True, self._item, self._playqueue, self._offset)
            self._player.wait_for_playback()
            self.__stop()
            self._playing = False

            if (self._play_wait == True):
                self._play_wait = False
                self.start()
            elif (self._next_index != None):
                self._offset = self._next_index
                self._next_index = None
                self.start()
            elif (self._stop_command == False and self._eof == True or self._next == True):
                self.__next()
            elif (self._prev == True):
                self.__prev()
            else:
                self.emit('media-playing', False, self._item, self._playqueue, self._offset)

    def prev(self):
        self._prev = True
        self._player.command('stop')

    def next(self):
        self._next = True
        self._player.command('stop')

    def __next(self):
        self.__playqueue_refresh()
        if (self._offset + 1 < len(self._playqueue.items)):
            self._offset = self._offset + 1
            self.start()
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset)

    def __prev(self):
        self.__playqueue_refresh()
        if (self._offset - 1 >= 0):
            self._offset = self._offset - 1
            self.start()
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset)

    def pause(self):
        self._player.pause = True

    def play(self):
        self._player.pause = False

    def stop(self):
        self._stop_command = True
        self._player.command('stop')

    def play_index(self, index):
        self._next_index = index
        self._player.command('stop')

    def __playqueue_refresh(self):
        self._playqueue.refresh()
        i = 0
        for item in self._playqueue.items:
            if item.playQueueItemID == self._playqueue.playQueueSelectedItemID:
                self._offset = i
            i += 1

    def __on_update(self, state='paused'):
        self._item.updateTimeline(self._progresUpdate * 1000, state=state, duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)
        if self._controller is not None:
            xml = self.get_timeline(state=state)
            print(xml)
            headers = {'Content-Type': 'application/xml', 'X-Plex-Client-Identifier': BASE_HEADERS['X-Plex-Client-Identifier']} # set what your server accepts
            print(requests.post(self._controller + '/:/timeline', data=xml, headers=headers).text)


    def get_timeline(self, state='paused'):
        up = urlparse(self._item._server._baseurl)
        prog = str(int(self._progresUpdate * 1000))
        return '''<?xml version: "1.0" encoding="UTF-8"?>
<MediaContainer location="navigation" commandID="''' + self._commandID + '''" machineIdentifier="''' + BASE_HEADERS['X-Plex-Client-Identifier'] + '''">
<Timeline type="video" itemType="video" state="stopped" controllable="playPause,stop,volume,audioStream,subtitleStream,seekTo,skipPrevious,skipNext,stepBack,stepForward" /><Timeline type="music" itemType="music" state="''' + state + '''" time="''' + prog + '''" duration="''' + str(self._item.duration) + '''" machineIdentifier="''' + self._item._server.machineIdentifier + '''" address="''' + up.hostname + '''" port="''' + str(up.port) + '''" protocol="''' + up.scheme + '''" token="''' + self._item._server._token + '''" containerKey="/playQueues/''' + str(self._playqueue.playQueueID) + '''" key="''' + self._item.key + '''" ratingKey="''' + str(self._item.ratingKey) + '''" playQueueID="''' + str(self._playqueue.playQueueID) + '''" playQueueItemID="''' + self._playqueue.playQueueSelectedItemID + '''" playQueueVersion="''' + str(self._playqueue.playQueueVersion) + '''" volume="100" controllable="playPause,stop,volume,shuffle,repeat,seekTo,skipPrevious,skipNext,stepBack,stepForward" /><Timeline type="photo" itemType="photo" state="stopped" controllable="skipPrevious,skipNext,stop" /></MediaContainer>'''

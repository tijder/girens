import gi
import mpv
import threading

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

    def __init__(self, **kwargs):
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

    def set_plex(self, plex):
        self._plex = plex

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
                self._item.updateTimeline(value * 1000, state='playing', duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)
            pass

        @self._player.property_observer('pause')
        def __on_pause(_name, value):
            self.emit('media-paused', value)
            if (value == True):
                self._item.updateTimeline(self._progresNow * 1000, state='paused', duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)

        @self._player.property_observer('eof-reached')
        def __on_eof(_name, value):
            if (value == True):
                self._eof = True
                self._player.command('stop')

        @self._player.property_observer('fullscreen')
        def __on_fullscreen(_name, value):
            self._fullscreen=value

    def __stop(self):
        self._item.updateTimeline(self._progresUpdate * 1000, state='stopped', duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)
        self._player.terminate()
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

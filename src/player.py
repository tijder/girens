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
                self._item.updateTimeline(value * 1000, state='playing', duration=self._item.duration)
            pass

        @self._player.property_observer('pause')
        def __on_pause(_name, value):
            self.emit('media-paused', value)
            if (value == True):
                self._item.updateTimeline(self._progresNow * 1000, state='paused', duration=self._item.duration)

        @self._player.property_observer('eof-reached')
        def __on_eof(_name, value):
            if (value == True):
                self._eof = True
                self._player.command('stop')

    def __stop(self):
        self._player.terminate()
        self.emit('media-playing', False, self._item, self._playqueue, self._offset)
        self._item.updateTimeline(self._progresNow * 1000, state='stopped', duration=self._item.duration)
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        self.__createPlayer()

    def set_playqueue(self, playqueue):
        self._playqueue = playqueue
        self._offset = int(self._playqueue.playQueueSelectedItemOffset)

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

            self._player.play(self._item.getStreamURL(offset=offset))
            self.emit('media-playing', True, self._item, self._playqueue, self._offset)
            self._player.wait_for_playback()
            self.__stop()
            self._playing = False

            if (self._play_wait == True):
                self._play_wait = False
                self.start()
            elif (self._stop_command == False and self._eof == True or self._next == True):
                self.__next()
            elif (self._prev == True):
                self.__prev()

    def prev(self):
        self._prev = True
        self._player.command('stop')

    def next(self):
        self._next = True
        self._player.command('stop')

    def __next(self):
        if (self._offset + 1 < len(self._playqueue.items)):
            self._offset = self._offset + 1
            self.start()

    def __prev(self):
        if (self._offset - 1 >= 0):
            self._offset = self._offset - 1
            self.start()

    def pause(self):
        self._player.pause = True

    def play(self):
        self._player.pause = False

    def stop(self):
        self._stop_command = True
        self._player.command('stop')

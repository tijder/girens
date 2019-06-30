import gi
import mpv
import threading
import time

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib, GdkPixbuf, GObject

# import gi
# gi.require_version('Gtk', '3.0')
# locale.setlocale(locale.LC_NUMERIC, 'C')
# player = mpv.MPV()

class Player(GObject.Object):
    __gsignals__ = {
        'playqueue-ended': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'video-starting': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'media-paused': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'media-playing': (GObject.SignalFlags.RUN_FIRST, None, (bool,object, object, int)),
        'media-time': (GObject.SignalFlags.RUN_FIRST, None, (int,)),
    }

    def __init__(self, resume_dialog, player_view, **kwargs):
        super().__init__(**kwargs)


        self._player_view = player_view

        self._resume_dialog = resume_dialog
        self._resume_dialog.connect("beginning-selected", self.__on_beginning_selected)
        self._resume_dialog.connect("resume-selected", self.__on_resume_selected)

        self._player = None
        self._progresUpdate = None
        self._progresNow = None
        self._item = None
        self._stop_command = False
        self._playing = False
        self._paused = False
        self._play_wait = False
        self._next_index = None
        self._fullscreen = False
        self._video_output_driver = "x11,"
        self._deinterlace = "no"

    def set_video_output_driver(self, video_output_driver):
        self._video_output_driver = video_output_driver

    def set_deinterlace(self, deinterlace):
        self._deinterlace = deinterlace

    def set_plex(self, plex):
        self._plex = plex

    def __createPlayer(self):
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        if self._item.listType == 'video':
            self._player = mpv.MPV(wid=str(self._player_view._frame.get_property("window").get_xid()), deinterlace=self._deinterlace, vo=self._video_output_driver, input_cursor="no", cursor_autohide="no", input_default_bindings="no")
        else:
            self._player = mpv.MPV(input_cursor="no", cursor_autohide="no", input_default_bindings="no")

        @self._player.property_observer('time-pos')
        def __time_observer(_name, value):
            self._progresNow = value
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
            self._paused = value
            self.emit('media-paused', value)
            if (value == True):
                self._item.updateTimeline(self._progresNow * 1000, state='paused', duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)

    def __stop(self):
        self._item.updateTimeline(self._progresUpdate * 1000, state='stopped', duration=self._item.duration, playQueueItemID=self._item.playQueueItemID)
        self._player.terminate()

    def set_playqueue(self, playqueue):
        self._playqueue = playqueue
        self.__playqueue_refresh()

    def start(self, from_beginning=None):
        new_item = self._playqueue.items[self._offset]
        if (self._playing != False):
            self._play_wait = True
            self._player.command('stop')
        elif new_item.viewOffset != 0 and from_beginning == None:
            GLib.idle_add(self.__ask_resume_or_beginning, new_item)
        else:
            self._item = new_item
            if self._item.listType == 'video':
                self.emit('video-starting')
                while self._player_view._frame.get_property("window") == None:
                    time.sleep(1)
            self._next = False
            self._prev = False
            self._eof = False
            self._playing = True
            self._stop_command = False
            self.__createPlayer()
            self._progresUpdate = 0
            self._progresNow = 0

            if (from_beginning == False):
                offset = self._item.viewOffset / 1000
            else:
                offset = 0

            source = self._plex.get_item_download_path(self._item)
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
            self._item = None
            self._playing = False

            if (self._play_wait == True):
                self._play_wait = False
                self.start()
            elif (self._next_index != None):
                self._offset = self._next_index
                self._next_index = None
                self.start()
            elif (self._prev == True):
                self.__prev()
            elif (self._stop_command == False or self._next == True):
                self.__next()
            else:
                self.emit('media-playing', False, self._item, self._playqueue, self._offset)
                self.emit('playqueue-ended')

    def prev(self):
        self._prev = True
        self._player.command('stop')

    def next(self):
        self._next = True
        self._player.command('stop')

    def get_position(self):
        return 0

    def get_state(self):
        if self._playing == False:
            return "Stopped"
        elif self._paused == True:
            return "Paused"
        else:
            return "Playing"

    def __next(self):
        self.__playqueue_refresh()
        if (self._offset + 1 < len(self._playqueue.items)):
            self._offset = self._offset + 1
            self.start()
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset)
            self.emit('playqueue-ended')

    def __prev(self):
        self.__playqueue_refresh()
        if (self._offset - 1 >= 0):
            self._offset = self._offset - 1
            self.start()
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset)
            self.emit('playqueue-ended')

    def play_pause(self):
        if self._player.pause:
            self.play()
        else:
            self.pause()

    def pause(self):
        self._player.pause = True

    def play(self):
        self._player.pause = False

    def stop(self):
        self._stop_command = True
        self._player.command('stop')

    def seek_backward(self):
        self._player.command('seek', -10)

    def seek_forward(self):
        self._player.command('seek', 30)

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

    def __ask_resume_or_beginning(self, item):
        self._resume_dialog.set_item(item)
        self._resume_dialog.show()

    def __on_beginning_selected(self, dialog, bool):
        thread = threading.Thread(target=self.start,kwargs={'from_beginning':True})
        thread.daemon = True
        thread.start()

    def __on_resume_selected(self, dialog, bool):
        thread = threading.Thread(target=self.start,kwargs={'from_beginning':False})
        thread.daemon = True
        thread.start()
        

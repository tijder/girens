import gi
import mpv
import threading
import time

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib, GdkPixbuf, GObject, Gio

# import gi
# gi.require_version('Gtk', '3.0')
# locale.setlocale(locale.LC_NUMERIC, 'C')
# player = mpv.MPV()

class Player(GObject.Object):
    __gsignals__ = {
        'playqueue-ended': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'playqueue-refreshed': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'video-starting': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'media-paused': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'media-playing': (GObject.SignalFlags.RUN_FIRST, None, (bool,object, object, int, object)),
        'media-time': (GObject.SignalFlags.RUN_FIRST, None, (int,)),
        'play-music-clip-instead-of-track': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
    }

    def __init__(self, resume_dialog, player_view, **kwargs):
        super().__init__(**kwargs)
        self._settings = Gio.Settings("nl.g4d.Girens")


        self._player_view = player_view

        self._resume_dialog = resume_dialog
        self._resume_dialog.connect("beginning-selected", self.__on_beginning_selected)
        self._resume_dialog.connect("resume-selected", self.__on_resume_selected)

        self._player = None
        self._playqueue = None
        self._progresUpdate = None
        self._lastInternUpdate = None
        self._progresNow = None
        self._item = None
        self._stop_command = False
        self._playing = False
        self._paused = False
        self._play_wait = False
        self._next_index = None
        self._fullscreen = False
        self._offset_param = None
        self._from_beginning = None
        self._playqueue_refreshed = False
        self._video_output_driver = "x11,"
        self._deinterlace = "no"
        self._play_music_clip_instead_of_track = False

    def set_video_output_driver(self, video_output_driver):
        self._video_output_driver = video_output_driver

    def set_deinterlace(self, deinterlace):
        self._deinterlace = deinterlace

    def set_plex(self, plex):
        self._plex = plex

    def __createPlayer(self, offset=0):
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        if self._item_loading.listType == 'video':
            self._player = mpv.MPV(wid=str(self._player_view._frame.get_property("window").get_xid()), deinterlace=self._deinterlace, vo=self._video_output_driver, input_cursor="no", cursor_autohide="no", input_default_bindings="no", sid="no", start=offset)
        else:
            self._player = mpv.MPV(input_cursor="no", cursor_autohide="no", input_default_bindings="no")

        @self._player.property_observer('time-pos')
        def __time_observer(_name, value):
            self._progresNow = value
            if value is not None and abs(value - self._lastInternUpdate) > 0.5:
                self._lastInternUpdate = value
                if (self._stop_command_given is False):
                    self.emit('media-time', value * 1000)
            if value is not None and abs(value - self._progresUpdate) > 5:
                self._progresUpdate = value
                self.__updateTimeline(value * 1000, state='playing', duration=self._item_loading.duration, playQueueItemID=self._item.playQueueItemID)
            pass

        @self._player.property_observer('pause')
        def __on_pause(_name, value):
            self._paused = value
            if (self._stop_command_given == False):
                self.emit('media-paused', value)
            if (value == True):
                self.__updateTimeline(self._progresNow * 1000, state='paused', duration=self._item_loading.duration, playQueueItemID=self._item.playQueueItemID)

        @self._player.property_observer('track-list')
        def __on_track_list(_name, value):
            plex_sub = self._item.getSelectedSubtitleStream()
            plex_audio = self._item.getSelectedAudioStream()

            for i in value:
                if plex_sub is not None and plex_sub.index is i['ff-index']:
                    self._player.sid = i['id']
                if plex_audio is not None and plex_audio.index is i['ff-index']:
                    self._player.aid = i['id']

    def __stop(self):
        self._item.updateTimeline(self._progresUpdate * 1000, state='stopped', duration=self._item_loading.duration, playQueueItemID=self._item.playQueueItemID)
        self._player.terminate()

    def set_playqueue(self, playqueue):
        self._playqueue = playqueue
        self.__playqueue_refresh()

    def start(self, from_beginning=None, offset_param=None):
        self._offset_param = None
        self._from_beginning = None
        new_item = self._playqueue.items[self._offset]
        if (self._playing != False):
            self._play_wait = True
            self._offset_param = offset_param
            self._from_beginning = from_beginning
            self._stop_command_given = True
            self._player.command('stop')
        elif new_item.viewOffset != 0 and from_beginning == None and offset_param == None:
            GLib.idle_add(self.__ask_resume_or_beginning, new_item)
        else:
            self._stop_command_given = False
            self._item = new_item
            self._item_loading = self._item
            if self._play_music_clip_instead_of_track and self._item.type == 'track' and self._item.primaryExtraKey != None:
                self._item_clip = self._plex._server.fetchItem(self._item.primaryExtraKey)
                self._item_loading = self._item_clip
            self._next = False
            self._prev = False
            self._eof = False
            self._playing = True
            self._stop_command = False
            self._playqueue_refreshed = False
            self._progresUpdate = 0
            self._lastInternUpdate = 0
            self._progresNow = 0

            if (from_beginning == False):
                offset = self._item_loading.viewOffset / 1000
            elif offset_param != None:
                offset = offset_param
            else:
                offset = 0

            source = self._plex.get_item_download_path(self._item_loading)
            if (source == None):
                direct = self._settings.get_boolean("play-media-direct")
                source = self._item_loading.getStreamURL(offset=offset, directPlay=direct)

            self.__createPlayer(offset=offset)
            self._player.volume = self._settings.get_int("volume-level")
            self._player.play(source)
            if self._item_loading.listType == 'video':
                sub = self._item.getSelectedSubtitleStream()
                if direct is True and sub is not None and sub.key is not None:
                    self._player.command('sub-add', sub.getDownloadUrl())
                thread = threading.Thread(target=self.emit,args={'video-starting'})
                thread.daemon = True
                thread.start()
            else:
                self.view_shown()
            self.__updateTimeline(0, state='playing', duration=self._item_loading.duration, playQueueItemID=self._item.playQueueItemID)
            thread = threading.Thread(target=self.__wait_for_playback)
            thread.daemon = True
            thread.start()

    def view_shown(self):
        thread = threading.Thread(target=self.emit,args=('media-playing', True, self._item, self._playqueue, self._offset, self._item_loading))
        thread.daemon = True
        thread.start()

    def start_with_params(self, from_beginning, offset_param):
        self.start(from_beginning=from_beginning, offset_param=offset_param)

    def __wait_for_playback(self):
        self._player.wait_for_playback()
        self.__stop()
        self._item = None
        self._item_loading = None
        self._playing = False

        if (self._play_wait == True):
            self._play_wait = False
            GLib.idle_add(self.start_with_params, self._from_beginning, self._offset_param)
        elif (self._next_index != None):
            self._offset = self._next_index
            self._next_index = None
            GLib.idle_add(self.start)
        elif (self._prev == True):
            self.__prev()
        elif (self._stop_command == False or self._next == True):
            self.__next()
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset, self._item_loading)
            self.emit('playqueue-ended')

    def prev(self):
        if (self._resume_dialog.is_visible()):
            self._resume_dialog.hide()
            self.__on_beginning_selected(None, True)
        elif (self._playing != False):
            self._prev = True
            self._stop_command_given = True
            self._player.command('stop')

    def next(self):
        if (self._resume_dialog.is_visible()):
            self._resume_dialog.hide()
            self.__on_resume_selected(None, False)
        elif (self._playing != False):
            self._next = True
            self._stop_command_given = True
            self._player.command('stop')

    def get_position(self):
        return self._progresNow

    def get_volume(self, percent=False):
        volume = self._settings.get_int("volume-level")
        if not percent:
            return volume / 100
        return volume

    def set_volume(self, percent):
        self._settings.set_int("volume-level", percent)
        if self._player and not self._player.playback_abort:
            self._player.volume = percent

    def get_state(self):
        if self._playing == False:
            return "Stopped"
        elif self._paused == True:
            return "Paused"
        else:
            return "Playing"

    def __next(self):
        if (self._offset + 1 < len(self._playqueue.items)):
            self._offset = self._offset + 1
            GLib.idle_add(self.start)
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset, self._item_loading)
            self.emit('playqueue-ended')

    def __prev(self):
        if (self._offset - 1 >= 0):
            self._offset = self._offset - 1
            GLib.idle_add(self.start)
        else:
            self.emit('media-playing', False, self._item, self._playqueue, self._offset, self._item_loading)
            self.emit('playqueue-ended')

    def has_next(self):
        return (self._offset + 1 < len(self._playqueue.items))

    def has_prev(self):
        return (self._offset - 1 >= 0)

    def has_media_item(self):
        if (self._playing == False):
            return False
        else:
            return True

    def play_pause(self):
        if (self._playing != False):
            if self._player.pause:
                self.play()
            else:
                self.pause()

    def pause(self):
        if (self._playing != False):
            self._player.pause = True

    def play(self):
        if (self._playing != False):
            self._player.pause = False

    def stop(self):
        if (self._playing != False):
            self._stop_command = True
            self._stop_command_given = True
            self._player.command('stop')

    def seek_backward(self):
        if (self._playing != False):
            self._player.command('seek', -10)

    def seek_forward(self):
        if (self._playing != False):
            self._player.command('seek', 30)

    def seek_to_time(self, time):
        self._player.seek(time, reference='absolute', precision='exact')

    def play_index(self, index):
        self._next_index = index
        self._stop_command_given = True
        self._player.command('stop')

    def play_from_key(self, key):
        if self._playqueue is not None:
            i = 0
            for item in self._playqueue.items:
                if (item.key == key):
                    self._next_index = i
                    self._stop_command_given = True
                    self._player.command('stop')
                i += 1

    def toggle_play_music_clip_instead_of_track(self):
        self._play_music_clip_instead_of_track = not self._play_music_clip_instead_of_track
        self.emit("play-music-clip-instead-of-track", self._play_music_clip_instead_of_track)

    def set_play_music_clip_instead_of_track(self, value):
        self._play_music_clip_instead_of_track = value

    def refresh_playqueue(self):
        self.__playqueue_refresh()

    def __playqueue_refresh(self):
        self._playqueue.refresh()
        i = 0
        for item in self._playqueue.items:
            if item.playQueueItemID == self._playqueue.playQueueSelectedItemID:
                self._offset = i
                playQueueSelectedItem = item
            i += 1
        self.emit("playqueue-refreshed", playQueueSelectedItem, self._playqueue)

    def __ask_resume_or_beginning(self, item):
        self._resume_dialog.set_item(item)
        self._resume_dialog.show()

    def __on_beginning_selected(self, dialog, bool):
        GLib.idle_add(self.start_with_params, True, None)

    def __on_resume_selected(self, dialog, bool):
        GLib.idle_add(self.start_with_params, False, None)
        
    def __updateTimeline(self, progres, state=None, duration=None, playQueueItemID=None):
        thread = threading.Thread(target=self._item.updateTimeline,args={progres},kwargs={'state':state,'duration':duration,'playQueueItemID':playQueueItemID})
        thread.daemon = True
        thread.start()
        if progres > 1000 and self._playqueue_refreshed == False:
            self._playqueue_refreshed = True
            thread = threading.Thread(target=self.__playqueue_refresh)
            thread.daemon = True
            thread.start()
        

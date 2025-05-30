from gi.repository import Gtk, GLib, Gio, GObject, Gdk, GdkPixbuf


from .cover_box import CoverBox
from .media_box import MediaBox
from .media_box_video_top import MediaBoxVideoTop
from .media_box_video_bottom import MediaBoxVideoBottom
from .item_bin import ItemBin

import time

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/player_view.ui')
class PlayerView(Gtk.ScrolledWindow):
    __gtype_name__ = 'player_view'

    __gsignals__ = {
        'fullscreen': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'windowed': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
    }

    _frame = Gtk.Template.Child()
    _overlay = Gtk.Template.Child()
    #_stack = Gtk.Template.Child()
    _controlls_top = Gtk.Template.Child()
    _controlls_bottom = Gtk.Template.Child()
    _video_box = Gtk.Template.Child()
    _box = Gtk.Template.Child()
    _revealer = Gtk.Template.Child()
    _label = Gtk.Template.Child()
    #_cover_image = Gtk.Template.Child()

    _title_label = Gtk.Template.Child()
    _release_datum_label = Gtk.Template.Child()
    _genre_label = Gtk.Template.Child()
    _duration_label = Gtk.Template.Child()
    _discription_label = Gtk.Template.Child()
    _subtitle_box = Gtk.Template.Child()
    _audio_box = Gtk.Template.Child()

    _card_box = Gtk.Template.Child()
    _options_list = Gtk.Template.Child()

    _deck_shows_box = Gtk.Template.Child()

    _download_key = None
    _download_thumb = None
    _item = None

    _timout = None
    _progress = 0
    _paused = True
    _fullscreen = False
    _windowed = False
    _playing = False

    _cover_width = 200
    _old_screensize = 640
    _width = 640

    _refreshing_item = False

    _last_button_click = 0

    _last_x = None
    _last_y = None

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

        self._subtitle_box.connect("notify::selected", self.__on_subtitle_selected)
        self._audio_box.connect("notify::selected", self.__on_audio_selected)

    def set_window(self, window):
        self._window = window

    def set_player(self, player):
        self._player = player
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("playqueue-ended", self.__on_playqueue_ended)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex.connect("section-shows-deck", self.__on_show_deck_update)
        self._deck_shows_box.set_plex(plex)

        if (self._player != None):
            self._media_box = MediaBox(self._plex, self._player, show_only_type="video")
            self._media_box_video_top = MediaBoxVideoTop()
            self._media_box_video_bottom = MediaBoxVideoBottom()
            self._media_box.set_video_top_ui(self._media_box_video_top)
            self._media_box.set_video_bottom_ui(self._media_box_video_bottom)

            self._controlls_top.append(self._media_box_video_top)
            self._controlls_bottom.append(self._media_box_video_bottom)
            self._controlls_top.set_visible(True)
            self._controlls_bottom.set_visible(True)
            self._media_box.connect("fullscreen-clicked", self.__on_fullscreen_button_clicked)
            self._media_box.connect("fullscreen-windowed-clicked", self.__on_fullscreen_windowed_button_clicked)
            self._media_box.connect("active", self.__on_media_box_active)

    def __on_fullscreen_button_clicked(self, button):
        self.__fullscreen()

    def __on_fullscreen_windowed_button_clicked(self, button):
        self.__toggle_windowed()

    def __toggle_windowed(self):
        if self._windowed:
            self.__add_extra_widgets()
        else:
            self.__remove_extra_widgets()
        self.emit('windowed', self._windowed)
        self._windowed = not self._windowed

    def __on_media_box_active(self, mediabox, active):
        if active:
            self.__stop_controlls_timout()
        else:
            self.__start_controlls_timout()

    def go_fullscreen(self):
        if (self._fullscreen == False):
            self.__fullscreen()

    def __fullscreen(self):
        if not self._fullscreen:
            self._old_screensize = self._window.get_width()
        self.emit('fullscreen', not self._fullscreen)

    def set_fullscreen_state(self):
        self._fullscreen = True
        self._media_box.hide_windowed_button()
        self.__remove_extra_widgets()

    def __remove_extra_widgets(self):
        self._revealer.set_reveal_child(False)
        self._video_box.set_vexpand(True)
        self._video_box.set_size_request(-1, -1)
        self.__show_controlls()
        self._controlls_top.get_style_context().add_class("black_background")

    def set_unfullscreen_state(self):
        self._fullscreen = False
        self._media_box.show_windowed_button()
        self._windowed = False
        self.__add_extra_widgets()

    def __add_extra_widgets(self):
        self.__set_correct_event_size(self._width)
        self._video_box.set_vexpand(False)
        self._revealer.set_reveal_child(True)
        self.__show_cursor()
        self._controlls_top.get_style_context().remove_class("black_background")

    def __on_playqueue_ended(self, player):
        if self._fullscreen == True:
            GLib.idle_add(self.__fullscreen)
        elif self._windowed == True:
            GLib.idle_add(self.__toggle_windowed)

    def __on_play_button_clicked(self, button):
        self._player.play_pause()

    @Gtk.Template.Callback()
    def on_keypress(self, widget, keyval, keycode, state):
        if keyval in [102, 65480]: # f and f11 key
            self.__fullscreen()
        elif keyval == 116: # t
            self.__toggle_windowed()
        elif keyval in [32, 112, 107]: # spacebar, p and    k
            self._player.play_pause()
        elif keyval == 111: # o
            self.__show_controlls()
        elif keyval == 113: # q
            self._player.stop()
        elif keyval == 44: # ,
            self._player.prev()
        elif keyval == 46: # .
            self._player.next()
        elif keyval == 109: # m
            self._player.toggle_play_music_clip_instead_of_track()
        elif keyval in [91, 65361]: # [ and left key
            self._player.seek_backward()
        elif keyval in [93, 65363]: # ] and right key
            self._player.seek_forward()
        elif keyval == 65307: # escape
            if (self._fullscreen == True):
                self.__fullscreen()

    def __show_cursor(self):
        self.set_cursor(Gdk.Cursor.new_from_name("default", None))

    def __hide_cursor(self):
        self.set_cursor(Gdk.Cursor.new_from_name("none", None))

    @Gtk.Template.Callback()
    def on_motion(self, widget, x, y):
        if (self._last_x != x and self._last_y != y):
            self._last_x = x
            self._last_y = y
            self.__show_controlls()

    def __stop_controlls_timout(self):
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

    def __start_controlls_timout(self):
        self._timout = GLib.timeout_add(3000, self.__on_motion_over)

    def __show_controlls(self):
        if self._playing == True and self._item.listType == 'video':
            GLib.idle_add(self._media_box.set_reveal_child, True)
        self.__stop_controlls_timout()
        self.__start_controlls_timout()
        if self._fullscreen:
            self.__show_cursor()

    @Gtk.Template.Callback()
    def on_button_press_event(self, widget, n_press, x, y):
        widget.set_state(Gtk.EventSequenceState.CLAIMED);
        if ((time.time() - self._last_button_click) < 0.4):
            self._last_button_click = 0
            self.__fullscreen()
        else:
            self._last_button_click = time.time()
            self.__show_controlls()

    @Gtk.Template.Callback()
    def on_right_click(self, widget, n_press, x, y):
        self._player.play_pause()

    def __on_motion_over(self):
        self._timout = None
        GLib.idle_add(self._media_box.set_reveal_child, False)
        if self._fullscreen:
            self.__hide_cursor()

    def __on_media_playing(self, player, playing, playqueue_item, playqueue, offset, item):
        self._playing = playing
        self._item = item

        if self._playing == True:
            self.__show_controlls()
            GLib.idle_add(self.__empty_flowbox)
            if self._item.type not in {'clip', 'track'}:
                thread = threading.Thread(target=self._plex.get_section_deck, args=(item.librarySectionID,))
                thread.daemon = True
                thread.start()
                GLib.idle_add(self.__set_box_visible, True)
            else:
                GLib.idle_add(self.__set_box_visible, False)

            if (not self._item.TYPE == 'playlist' and not self._item.TYPE == 'episode'):
                self._download_key = self._item.ratingKey
                self._download_thumb = self._item.thumb
            elif (self._item.type == 'playlist'):
                self._download_key = self._item.ratingKey
                self._download_thumb = self._item.composite
            else:
                self._download_key = self._item.grandparentRatingKey
                self._download_thumb = self._item.grandparentThumb

            genre_title = ''
            if hasattr(self._item, 'genres'):
                for genre in self._item.genres:
                    genre_title = genre_title + genre.tag + " "

            release_datum = ''
            if hasattr(self._item, 'originallyAvailableAt') and self._item.originallyAvailableAt != None:
                release_datum = str(self._item.originallyAvailableAt.strftime('%d %b %Y'))

            self.__set_list_box_item(self._title_label, self._item.title)
            self.__set_list_box_item(self._release_datum_label, release_datum)
            self.__set_list_box_item(self._genre_label, genre_title)
            self.__set_list_box_item(self._duration_label, str(self.__convertMillis(self._item.duration)))
            self.__set_list_box_item(self._discription_label, self._item.summary)

            GLib.idle_add(self.__set_stream_widgets)

            thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
            thread.daemon = True
            thread.start()

    def __set_list_box_item(self, box, title):
        if title is not None and title != '':
            box.set_title(title)
            box.set_visible(True)
        else:
            box.set_visible(False)


    def __set_stream_widgets(self):
        self._refreshing_item = True
        self._selected_subtitle_stream = None
        self._sub_store = Gtk.StringList()
        sub_selected = 0
        self._selected_audio_stream = None
        self._audio_store = Gtk.StringList()
        audio_selected = -1

        itemBin = ItemBin()
        itemBin.set_item([None, _('None')])
        self._sub_store.append(_('None'))

        i = 0
        y = 0
        for parts in self._item.iterParts():
            for stream in parts.subtitleStreams():
                i = i + 1
                itemBin = ItemBin()
                itemBin.set_item([stream, stream.displayTitle])
                self._sub_store.append(stream.displayTitle)
                if stream.selected is True:
                    self._selected_subtitle_stream = stream
                    sub_selected = i

            for stream in parts.audioStreams():
                itemBin = ItemBin()
                itemBin.set_item([stream, stream.displayTitle])
                self._audio_store.append(stream.displayTitle)
                if stream.selected is True:
                    self._selected_audio_stream = stream
                    audio_selected = y
                y = y + 1
            break

        self.__set_combobox(self._subtitle_box, self._sub_store, sub_selected)
        self.__set_combobox(self._audio_box, self._audio_store, audio_selected)

        self._subtitle_box.set_visible(len(self._sub_store) > 1)
        self._audio_box.set_visible(len(self._audio_store) > 1)
        self._options_list.set_visible(len(self._audio_store) > 1 or len(self._sub_store) > 1)
        self._refreshing_item = False

    def __set_combobox(self, box, store, selected):
        box.set_model(store)
        box.set_selected(selected)
        renderer_text = Gtk.CellRendererText()
        box.set_visible(len(store) > 1)

    def __on_subtitle_selected(self, widget, param):
        i = 0
        if self._subtitle_box.get_selected() == 0:
            self.__on_process_slected(None, 'subtitle')
        else:
            for parts in self._item.iterParts():
                for stream in parts.subtitleStreams():
                    i = i + 1
                    if i is self._subtitle_box.get_selected():
                        self.__on_process_slected(stream, 'subtitle')

    def __on_audio_selected(self, widget, param):
        i = 0
        for parts in self._item.iterParts():
            for stream in parts.audioStreams():
                if (i is self._audio_box.get_selected()):
                    self.__on_process_slected(stream, 'audio')
                i = i + 1

    def __on_process_slected(self, stream, what):
        if what == 'audio':
            current_stream = self._selected_audio_stream
        elif what == 'subtitle':
            current_stream = self._selected_subtitle_stream

        if stream is not current_stream and self._refreshing_item is False:
            if what == 'audio':
                self._selected_audio_stream = stream
                self._player.set_audio(stream)
            elif what == 'subtitle':
                self._selected_subtitle_stream = stream
                self._player.set_subtitle(stream)

    def __set_box_visible(self, booleon):
        self._label.set_visible(booleon)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 300, 300)
            #GLib.idle_add(self.__set_image, pix)

    #def __set_image(self, pix):
    #    self._cover_image.set_from_pixbuf(pix)

    def __empty_flowbox(self):
        self._deck_shows_box.empty_list()

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_list, self._deck_shows_box, item)

    def __add_to_list(self, hub, item):
        item_bin = ItemBin()
        item_bin.set_item(item)
        hub.add_item(item_bin)

    def width_changed(self, width):
        self._width = width
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200
        self._deck_shows_box.set_cover_width(self._cover_width)

        if width < 850:
            self._card_box.set_orientation(Gtk.Orientation.VERTICAL)
        else:
            self._card_box.set_orientation(Gtk.Orientation.HORIZONTAL)


        if not self._windowed:
            self.__set_correct_event_size(width)

    def __set_correct_event_size(self, width):
        if width < 850:
            self._video_box.set_size_request(-1, 300)
        elif width < 1500:
            self._video_box.set_size_request(-1, 500)
        else:
            self._video_box.set_size_request(-1, 800)

    def __convertMillis(self, millis):
        seconds=(millis/1000)%60
        minutes=(millis/(1000*60))%60
        hours=(millis/(1000*60*60))%24
        text = ''
        if hours > 1:
            print(hours)
            text = str("{0} hr ".format(int(hours)))
        if minutes > 1:
            text = text + str("{0} min".format(int(minutes)))
        return text


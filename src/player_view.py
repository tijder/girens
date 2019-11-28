from gi.repository import Gtk, GLib, Gio, GObject, Gdk, GdkPixbuf
from .gi_composites import GtkTemplate

from .cover_box import CoverBox
from .media_box import MediaBox
from .media_box_video_top import MediaBoxVideoTop
from .media_box_video_bottom import MediaBoxVideoBottom

import threading

@GtkTemplate(ui='/nl/g4d/Girens/player_view.ui')
class PlayerView(Gtk.Box):
    __gtype_name__ = 'player_view'

    __gsignals__ = {
        'fullscreen': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-album-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _frame = GtkTemplate.Child()
    _overlay = GtkTemplate.Child()
    _stack = GtkTemplate.Child()
    _controlls_top = GtkTemplate.Child()
    _controlls_bottom = GtkTemplate.Child()
    _event = GtkTemplate.Child()
    _box = GtkTemplate.Child()
    _label = GtkTemplate.Child()
    _cover_image = GtkTemplate.Child()

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _left_subtitle_label = GtkTemplate.Child()
    _right_subtitle_label = GtkTemplate.Child()
    _discription_label = GtkTemplate.Child()
    _subtitle_box = GtkTemplate.Child()
    _sub_label = GtkTemplate.Child()
    _audio_box = GtkTemplate.Child()
    _audio_label = GtkTemplate.Child()

    _deck_shows_box = GtkTemplate.Child()

    _download_key = None
    _download_thumb = None
    _item = None

    _timout = None
    _progress = 0
    _paused = True
    _fullscreen = False
    _playing = False

    _cover_width = 200
    _old_screensize = 640

    def __init__(self, window, **kwargs):
        super().__init__(**kwargs)
        self.init_template()
        self._window = window

        self._event.add_events(Gdk.EventMask.POINTER_MOTION_MASK)
        self._event.connect("motion-notify-event", self.__on_motion)
        self._subtitle_box.connect("changed", self.__on_subtitle_selected)
        self._audio_box.connect("changed", self.__on_audio_selected)
        window.connect("key-press-event", self.__on_keypress)

    def set_player(self, player):
        self._player = player
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("playqueue-ended", self.__on_playqueue_ended)

    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex.connect("section-shows-deck", self.__on_show_deck_update)

        if (self._player != None):
            self._media_box = MediaBox(self._plex, self._player, show_only_type="video")
            self._media_box_video_top = MediaBoxVideoTop()
            self._media_box_video_bottom = MediaBoxVideoBottom()
            self._media_box.set_video_top_ui(self._media_box_video_top)
            self._media_box.set_video_bottom_ui(self._media_box_video_bottom)

            self._controlls_top.add(self._media_box_video_top)
            self._controlls_bottom.add(self._media_box_video_bottom)
            self._controlls_top.set_visible(True)
            self._controlls_bottom.set_visible(True)
            self._media_box.connect("fullscreen-clicked", self.__on_fullscreen_button_clicked)
            self._media_box.connect("active", self.__on_media_box_active)

    def __on_fullscreen_button_clicked(self, button):
        self.__fullscreen()

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
            self._old_screensize = self._window.get_size()[0]
        self.emit('fullscreen', not self._fullscreen)

    def set_fullscreen_state(self):
        self._fullscreen = True
        self._box.hide()
        self._event.set_vexpand(True)
        self._event.set_size_request(-1, -1)
        self.__show_controlls()
        self._controlls_top.get_style_context().add_class("black_background")

    def set_unfullscreen_state(self):
        self._fullscreen = False
        self.__set_correct_event_size(self._old_screensize)
        self._box.show()
        self._event.set_vexpand(False)
        #self._event.set_size_request(-1, 500)
        self.__show_cursor()
        self._controlls_top.get_style_context().remove_class("black_background")

    def __on_playqueue_ended(self, player):
        if self._fullscreen == True:
            GLib.idle_add(self.__fullscreen)

    def __on_play_button_clicked(self, button):
        self._player.play_pause()


    def __on_keypress(self, widget, key):
        if key.keyval in [102, 65480]: # f and f11 key
            self.__fullscreen()
        elif key.keyval in [32, 112]: # spacebar and p
            self._player.play_pause()
        elif key.string == 'o':
            self.__show_controlls()
        elif key.string == 'q':
            self._player.stop()
        elif key.string == ',':
            self._player.prev()
        elif key.string == '.':
            self._player.next()
        elif key.string == 'm':
            self._player.toggle_play_music_clip_instead_of_track()
        elif key.keyval in [91, 65361]: # [ and left key
            self._player.seek_backward()
        elif key.keyval in [93, 65363]: # ] and right key
            self._player.seek_forward()

    def __show_cursor(self):
        Gdk.Window.set_cursor(self.get_window(), Gdk.Cursor.new_from_name(Gdk.Display.get_default(),"default"))

    def __hide_cursor(self):
        Gdk.Window.set_cursor(self.get_window(), Gdk.Cursor.new_from_name(Gdk.Display.get_default(),"none"))

    def __on_motion(self, widget, motion):
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

            title = ''
            subtitle = ''
            leftsubtitle = ''
            rightsubtitle = ''
            sumary = ''

            if (self._item.TYPE == 'episode'):
                title = self._item.grandparentTitle
                subtitle = self._item.seasonEpisode + ' - ' + self._item.title
                rightsubtitle = str(self._item.originallyAvailableAt.strftime('%d %b %Y'))
            elif (self._item.TYPE == 'movie'):
                title = self._item.title
                subtitle = str(self._item.year)
                for genre in self._item.genres:
                    rightsubtitle = rightsubtitle + genre.tag + " "
            if self._item.TYPE in ['movie', 'episode']:
                leftsubtitle = str(self.__convertMillis(self._item.duration))
                sumary = self._item.summary

            self._title_label.set_text(title)
            self._subtitle_label.set_text(subtitle)
            self._left_subtitle_label.set_text(leftsubtitle)
            self._right_subtitle_label.set_text(rightsubtitle)
            self._discription_label.set_text(sumary)

            GLib.idle_add(self.__set_stream_widgets)

            thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
            thread.daemon = True
            thread.start()

    def __set_stream_widgets(self):
        self._selected_subtitle_stream = None
        self._sub_store = Gtk.ListStore(object, str)
        sub_selected = -1
        self._selected_audio_stream = None
        self._audio_store = Gtk.ListStore(object, str)
        audio_selected = -1

        self._sub_store.append([None, 'None'])

        i = 0
        y = 0
        for parts in self._item.iterParts():
            for stream in parts.subtitleStreams():
                i = i + 1
                self._sub_store.append([stream, stream.displayTitle])
                if stream.selected is True:
                    self._selected_subtitle_stream = stream
                    sub_selected = i

            for stream in parts.audioStreams():
                self._audio_store.append([stream, stream.displayTitle])
                if stream.selected is True:
                    self._selected_audio_stream = stream
                    audio_selected = y
                y = y + 1

        self.__set_combobox(self._subtitle_box, self._sub_store, sub_selected)
        self.__set_combobox(self._audio_box, self._audio_store, audio_selected)

        self._sub_label.set_visible(len(self._sub_store) > 1)
        self._audio_label.set_visible(len(self._audio_store) > 1)

    def __set_combobox(self, box, store, selected):
        box.clear()
        box.set_model(store)
        box.set_id_column(1)
        renderer_text = Gtk.CellRendererText()
        box.pack_start(renderer_text, True)
        box.add_attribute(renderer_text, "text", 1)
        box.set_active(selected)
        box.set_visible(len(store) > 1)

    def __on_subtitle_selected(self, combo):
        self.__on_process_slected(combo, 'subtitle')

    def __on_audio_selected(self, combo):
        self.__on_process_slected(combo, 'audio')

    def __on_process_slected(self, combo, what):
        if what is 'audio':
            current_stream = self._selected_audio_stream
        elif what is 'subtitle':
            current_stream = self._selected_subtitle_stream

        tree_iter = combo.get_active_iter()
        if tree_iter is not None:
            model = combo.get_model()
            stream, stream_name = model[tree_iter][:2]
            if (current_stream is None or stream is not current_stream):
                if what is 'audio':
                    self._selected_audio_stream = stream
                    self._player.set_audio(stream)
                elif what is 'subtitle':
                    self._selected_subtitle_stream = stream
                    self._player.set_subtitle(stream)

    def __set_box_visible(self, booleon):
        self._label.set_visible(booleon)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 300, 300)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

    def __empty_flowbox(self):
        for item in self._deck_shows_box.get_children():
            self._deck_shows_box.remove(item)

    def __on_show_deck_update(self, plex, items):
        for item in items:
            GLib.idle_add(self.__add_to_hub, self._deck_shows_box, item)

    def __add_to_hub(self, hub, item):
        cover = CoverBox(self._plex, item, cover_width=self._cover_width)
        cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
        cover.connect("view-album-wanted", self.__on_go_to_album_clicked)
        cover.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
        hub.add(cover)

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)

    def __on_go_to_album_clicked(self, cover, key):
        self.emit('view-album-wanted', key)

    def __on_go_to_artist_clicked(self, cover, key):
        self.emit('view-artist-wanted', key)

    def width_changed(self, width):
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200

        self.__set_correct_event_size(width)

    def __set_correct_event_size(self, width):
        if width < 850:
            self._event.set_size_request(-1, 300)
        elif width < 1500:
            self._event.set_size_request(-1, 500)
        else:
            self._event.set_size_request(-1, 800)

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

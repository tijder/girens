# window.py
#
# Copyright 2018 Gerben Droogers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk
from .gi_composites import GtkTemplate
from .cover_box import CoverBox
from .playqueue_popover import PlayqueuePopover

import threading

class MediaBox(GObject.Object):
    __gtype_name__ = 'media_box'

    __gsignals__ = {
        'fullscreen-clicked': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'active': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
    }

    _close_button = None
    _play_button = None
    _prev_button = None
    _next_button = None
    _skip_backward_button = None
    _skip_forward_button = None
    _play_image = None
    _fullscreen_button = None

    _title_label = None
    _subtitle_label = None
    _scale_bar = None
    _scale_adjustment = None
    _progress_bar = None
    _time_left_label = None
    _time_right_label = None

    _playqueue_button = None
    _cover_image = None

    _item = None
    _paused = False
    _playing = False
    _progress = 0
    _fraction = 0

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, player, show_only_type="audio", **kwargs):
        super().__init__(**kwargs)

        self._plex = plex
        self._player = player

        self._show_only_type = show_only_type

        self._boxes = []

        self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._player.connect("media-paused", self.__on_media_paused)
        self._player.connect("media-playing", self.__on_media_playing)
        self._player.connect("media-time", self.__on_media_time)

    def set_visible(self, booleon):
        for box in self._boxes:
            box.set_visible(booleon)

    def set_reveal_child(self, booleon):
        for box in self._boxes:
            box.set_reveal_child(booleon)


    def set_music_ui(self, box):
        self._boxes.append(box)
        box.set_visible(True)
        self.__set_playque_button(box._playqueue_button)
        self.__set_play_button(box._play_button)
        self.__set_prev_button(box._prev_button)
        self.__set_next_button(box._next_button)
        self.__set_close_button(box._close_button)
        self.__set_scale_bar(box._scale_bar)
        self.__set_scale_adjustment(box._scale_adjustment)
        self.__set_title_label(box._title_label)
        self.__set_subtitle_label(box._subtitle_label)
        self.__set_play_image(box._play_image)
        self.__set_cover_image(box._cover_image)
        self.__set_time_left_label(box._time_left_label)
        self.__set_time_right_label(box._time_right_label)

        self.set_reveal_child(False)

    def set_video_top_ui(self, box):
        self._boxes.append(box)
        box.set_visible(True)
        self.__set_fullscreen_button(box._fullscreen_button)
        self.__set_close_button(box._close_button)
        self.__set_title_label(box._title_label)
        self.__set_subtitle_label(box._subtitle_label)

        self.set_reveal_child(False)

    def set_video_bottom_ui(self, box):
        self._boxes.append(box)
        box.set_visible(True)
        self.__set_playque_button(box._playqueue_button)
        self.__set_play_button(box._play_button)
        self.__set_prev_button(box._prev_button)
        self.__set_next_button(box._next_button)
        self.__set_progress_bar(box._progress_bar)
        self.__set_play_image(box._play_image)
        self.__set_cover_image(box._cover_image)
        self.__set_skip_backward_button(box._skip_backward_button)
        self.__set_skip_forward_button(box._skip_forward_button)
        self.__set_time_left_label(box._time_left_label)
        self.__set_time_right_label(box._time_right_label)

        self.set_reveal_child(False)

    def __set_playque_button(self, button):
        self._playqueue_button = button
        self._playqueue_popover = PlayqueuePopover(self._plex, self._player)
        self._playqueue_button.set_popover(self._playqueue_popover)

        self._playqueue_popover.connect("show-button", self.__on_playqueue_show_button)
        self._playqueue_popover.connect("show", self.__on_playqueue_show)
        self._playqueue_popover.connect("hide", self.__on_playqueue_hide)

    def __set_skip_backward_button(self, button):
        self._skip_backward_button = button
        self._skip_backward_button.connect("clicked", self.__on_skip_backward_button_clicked)

    def __set_skip_forward_button(self, button):
        self._skip_forward_button = button
        self._skip_forward_button.connect("clicked", self.__on_skip_forward_button_clicked)

    def __set_time_left_label(self, label):
        self._time_left_label = label

    def __set_time_right_label(self, label):
        self._time_right_label = label

    def __set_play_button(self, button):
        self._play_button = button
        self._play_button.connect("clicked", self.__on_play_button_clicked)

    def __set_prev_button(self, button):
        self._prev_button = button
        self._prev_button.connect("clicked", self.__on_prev_button_clicked)

    def __set_next_button(self, button):
        self._next_button = button
        self._next_button.connect("clicked", self.__on_next_button_clicked)

    def __set_close_button(self, button):
        self._close_button = button
        self._close_button.connect("clicked", self.__on_close_button_clicked)

    def __set_fullscreen_button(self, button):
        self._fullscreen_button = button
        self._fullscreen_button.connect("clicked", self.__on_fullscreen_button_clicked)

    def __set_progress_bar(self, bar):
        self._progress_bar = bar

    def __set_scale_bar(self, bar):
        self._scale_bar = bar

    def __set_scale_adjustment(self, adjustment):
        self._scale_adjustment = adjustment
        self._scale_adjustment.connect("value_changed", self.__on_scale_time_change)

    def __set_title_label(self, label):
        self._title_label = label

    def __set_subtitle_label(self, label):
        self._subtitle_label = label

    def __set_play_image(self, image):
        self._play_image = image

    def __set_cover_image(self, image):
        self._cover_image = image

    def __on_skip_forward_button_clicked(self, button):
        self._player.seek_forward()

    def __on_skip_backward_button_clicked(self, button):
        self._player.seek_backward()

    def __on_play_button_clicked(self, button):
        self._player.play_pause()

    def __on_close_button_clicked(self, button):
        self._player.stop()

    def __on_fullscreen_button_clicked(self, button):
        self.emit('fullscreen-clicked')

    def __on_prev_button_clicked(self, button):
        thread = threading.Thread(target=self._player.prev)
        thread.daemon = True
        thread.start()

    def __on_next_button_clicked(self, button):
        thread = threading.Thread(target=self._player.next)
        thread.daemon = True
        thread.start()

    def __on_media_paused(self, player, paused):
        self._paused = paused
        GLib.idle_add(self.__update_buttons)

    def __on_media_playing(self, player, playing, playqueue_item, playqueue, offset, item):
        if item != None and item.listType == self._show_only_type:
            self.__update_media_playing(player, playing, item, playqueue, offset)
            self.set_reveal_child(playing)
        else:
            self.set_reveal_child(False)

    def __update_media_playing(self, player, playing, item, playqueue, offset):
        self._playing = playing
        self._item = item
        self._playqueue = playqueue
        self._offset = offset

        if playing == True:
            self._progress = 0

        if (playing == True and self._cover_image != None):
            self.__reload_image()

        GLib.idle_add(self.__update_buttons)

    def __reload_image(self):
        if (not self._item.TYPE == 'playlist'):
            self._download_key = self._item.ratingKey
            self._download_thumb = self._item.thumb
        elif (self._item.type == 'playlist'):
            self._download_key = self._item.ratingKey
            self._download_thumb = self._item.composite

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def __on_media_time(self, player, time):
        self._progress = time
        GLib.idle_add(self.__update_buttons)

    def __update_play_image_icon(self, string, number):
        if self._play_image != None:
            self._play_image.set_from_icon_name(string, number)

    def __updat_time_left_label(self, string):
        if self._time_left_label != None:
            self._time_left_label.set_text(str(string))

    def __updat_time_right_label(self, string):
        if self._time_right_label != None:
            self._time_right_label.set_text(str(string))

    def __update_progress_bar(self, fraction):
        if self._progress_bar != None:
            self._progress_bar.set_fraction(fraction)
        if self._scale_adjustment != None:
            self._scale_adjustment.set_value(fraction)

    def __convertMillis(self, millis):
        string = ""
        seconds=format(int((millis/1000)%60), '02')
        minutes=format(int((millis/(1000*60))%60), '02')
        hours=format(int((millis/(1000*60*60))%24), '02')

        if hours != "00":
            string += str(hours) + ":"
        string += str(minutes) + ":" + str(seconds)
        return string

    def __update_buttons(self):
        if (self._paused == True):
            self.__update_play_image_icon('media-playback-start-symbolic', 4)
        else:
            self.__update_play_image_icon('media-playback-pause-symbolic', 4)

        if (self._item != None):
            self._prev_button.set_sensitive(self._offset - 1 >= 0)
            self._next_button.set_sensitive(self._offset + 1 < len(self._playqueue.items))
            self._fraction = self._progress / self._item.duration
            self.__update_progress_bar(self._fraction)

            self.__updat_time_left_label(self.__convertMillis(self._progress))
            self.__updat_time_right_label("-" + self.__convertMillis(self._item.duration - self._progress))

            title = ''
            subtitle = ''

            if (self._item.TYPE == 'episode'):
                title = self._item.grandparentTitle
                subtitle = self._item.seasonEpisode + ' - ' + self._item.title
            elif (self._item.TYPE == 'movie'):
                title = self._item.title
                subtitle = str(self._item.year)
            elif (self._item.TYPE == 'track'):
                title = self._item.title
                subtitle = self._item.grandparentTitle + ' - ' + self._item.parentTitle

            self._title_label.set_text(title)
            self._subtitle_label.set_text(subtitle)

    def __on_scale_time_change(self, scale):
        value = scale.get_value()
        if self._item != None and self._fraction != value:
            self._player.seek_to_time(value * self._item.duration / 1000)

    def __on_playqueue_show_button(self, playqueue):
        self._playqueue_button.set_active(False)

    def __on_playqueue_show(self, playqueue):
        self.emit('active', True)

    def __on_playqueue_hide(self, playqueue):
        self.emit('active', False)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 50, 50)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._cover_image.set_from_pixbuf(pix)

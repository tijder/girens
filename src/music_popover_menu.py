# cover_box.py
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


@Gtk.Template(resource_path='/nl/g4d/Girens/music_popover_menu.ui')
class MusicPopoverMenu(Gtk.Popover):
    __gtype_name__ = 'music_popover_menu'

    _current_item = None

    _star_1_button = Gtk.Template.Child()
    _star_1_img = Gtk.Template.Child()

    _star_2_button = Gtk.Template.Child()
    _star_2_img = Gtk.Template.Child()

    _star_3_button = Gtk.Template.Child()
    _star_3_img = Gtk.Template.Child()

    _star_4_button = Gtk.Template.Child()
    _star_4_img = Gtk.Template.Child()

    _star_5_button = Gtk.Template.Child()
    _star_5_img = Gtk.Template.Child()

    _shuffle_button = Gtk.Template.Child()
    _repeat_button = Gtk.Template.Child()

    def __init__(self, player, **kwargs):
        super().__init__(**kwargs)


        self._player = player
        self._player.connect("media-playing", self.__on_media_playing)

        self._star_1_button.connect("clicked", self.__star_1_clicked)
        self._star_2_button.connect("clicked", self.__star_2_clicked)
        self._star_3_button.connect("clicked", self.__star_3_clicked)
        self._star_4_button.connect("clicked", self.__star_4_clicked)
        self._star_5_button.connect("clicked", self.__star_5_clicked)

        self._shuffle_button.connect("state-set", self.__shuffle_button_clicked)
        self._repeat_button.connect("state-set", self.__repeat_button_clicked)

    def __shuffle_button_clicked(self, button, state):
        if (state == True):
            self._player.shuffle()
        else:
            self._player.unshuffle()

    def __repeat_button_clicked(self, button, state):
        self._player.set_repeat(state)

    def __set_shuffle_state(self, state):
        GLib.idle_add(self._shuffle_button.set_active, state)

    def __on_media_playing(self, player, playing, item, playqueue, offset, item_loaded):
        if (item != None and item.listType == 'audio'):
            self.__set_stars(int(round(item.userRating/2)))
            self._current_item = item
            self.__set_shuffle_state(playqueue.playQueueShuffled)
            GLib.idle_add(self._repeat_button.set_active, False)

    def __set_item_stars(self, rating):
        if (self._current_item != None):
            self._current_item.rate(rating * 2)

    def __star_1_clicked(self, button):
        self.__set_stars(1)
        self.__set_item_stars(1)

    def __star_2_clicked(self, button):
        self.__set_stars(2)
        self.__set_item_stars(2)

    def __star_3_clicked(self, button):
        self.__set_stars(3)
        self.__set_item_stars(3)

    def __star_4_clicked(self, button):
        self.__set_stars(4)
        self.__set_item_stars(4)

    def __star_5_clicked(self, button):
        self.__set_stars(5)
        self.__set_item_stars(5)

    def __set_stars(self, stars):
        if (stars >= 1):
            GLib.idle_add(self._star_1_img.set_from_icon_name, 'starred-symbolic')
        else:
            GLib.idle_add(self._star_1_img.set_from_icon_name, 'non-starred-symbolic')

        if (stars >= 2):
            GLib.idle_add(self._star_2_img.set_from_icon_name, 'starred-symbolic')
        else:
            GLib.idle_add(self._star_2_img.set_from_icon_name, 'non-starred-symbolic')

        if (stars >= 3):
            GLib.idle_add(self._star_3_img.set_from_icon_name, 'starred-symbolic')
        else:
            GLib.idle_add(self._star_3_img.set_from_icon_name, 'non-starred-symbolic')

        if (stars >= 4):
            GLib.idle_add(self._star_4_img.set_from_icon_name, 'starred-symbolic')
        else:
            GLib.idle_add(self._star_4_img.set_from_icon_name, 'non-starred-symbolic')

        if (stars >= 5):
            GLib.idle_add(self._star_5_img.set_from_icon_name, 'starred-symbolic')
        else:
            GLib.idle_add(self._star_5_img.set_from_icon_name, 'non-starred-symbolic')

        

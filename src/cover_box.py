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

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk, Gio


from .sync_settings import SyncSettings

import cairo
import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/cover_box.ui')
class CoverBox(Gtk.Button):
    __gtype_name__ = 'cover_box'

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()
    _progress_bar = Gtk.Template.Child()
    _watched_image = Gtk.Template.Child()
    _cover_image = Gtk.Template.Child()

    popover_menu = Gtk.Template.Child()

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, show_view=False, cover_width=200, **kwargs):
        super().__init__(**kwargs)

        self._plex = plex
        self._cover_width = cover_width
        self._show_view = show_view
        self._plex_connect_id = self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex_retrieved_id = self._plex.connect("item-retrieved", self.__on_item_retrieved)

    def set_item(self, item):
        self._item = item
        self.__set_item(self._item)

        target = ""
        target_value = None

        if item.TYPE in ['episode', 'movie', 'playlist']:
            target = "win.play-item"
            target_value = GLib.Variant.new_int64(self._item.ratingKey)
        elif item.TYPE in ['season', 'show']:
            if self._item.TYPE == 'season':
                target_value = GLib.Variant.new_int64(self._item.parentRatingKey)
            elif self._item.TYPE == 'show':
                target_value = GLib.Variant.new_int64(self._item.ratingKey)
            target = "win.show-show-by-id"
        elif self._item.TYPE == 'artist':
            target_value = GLib.Variant.new_int64(self._item.ratingKey)
            target = "win.show-artist-by-id"
        elif self._item.TYPE == 'album':
            target_value = GLib.Variant.new_int64(int(self._item.ratingKey))
            target = "win.show-album-by-id"


        self.set_action_target_value(target_value)
        self.set_action_name(target)

        if (not item.TYPE == 'playlist' and (not item.TYPE == 'episode' or self._show_view)):
            self._download_key = item.ratingKey
            self._download_thumb = item.thumb
        elif (item.type == 'playlist'):
            self._download_key = item.ratingKey
            self._download_thumb = item.composite
        else:
            self._download_key = item.grandparentRatingKey
            self._download_thumb = item.grandparentThumb

        thread = threading.Thread(target=self._plex.download_cover, args=(self._download_key, self._download_thumb))
        thread.daemon = True
        thread.start()

    def __set_item(self, item):
        menu = Gio.Menu()

        if self._item.TYPE == 'album':
            menu_item = Gio.MenuItem.new(_("Go to album view"), "win.show-album-by-id")
            menu_item.set_action_and_target_value("win.show-album-by-id", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if self._item.TYPE in ['artist', 'album']:
            if self._item.TYPE == 'artist':
                param_key = GLib.Variant.new_int64(self._item.ratingKey)
            if self._item.TYPE == 'album':
                param_key = GLib.Variant.new_int64(int(self._item.parentRatingKey))
            menu_item = Gio.MenuItem.new(_("Go to artist view"), "win.show-artist-by-id")
            menu_item.set_action_and_target_value("win.show-artist-by-id", param_key)
            menu.append_item(menu_item)

        if (item.TYPE in ['episode', 'season', 'show']):
            if (not self._show_view):
                if self._item.TYPE == 'episode':
                    param_key = GLib.Variant.new_int64(self._item.grandparentRatingKey)
                elif self._item.TYPE == 'season':
                    param_key = GLib.Variant.new_int64(self._item.parentRatingKey)
                elif self._item.TYPE == 'show':
                    param_key = GLib.Variant.new_int64(self._item.ratingKey)
                menu_item = Gio.MenuItem.new(_("Go to show view"), "win.show-show-by-id")
                menu_item.set_action_and_target_value("win.show-show-by-id", param_key)
                menu.append_item(menu_item)

        if item.TYPE in ['episode', 'season', 'show', 'movie'] and (not item.isWatched or item.isWatched and hasattr(item, 'viewOffset') and item.viewOffset != 0):
            menu_item = Gio.MenuItem.new(_("Mark as Played"), "win.mark-as-played-by-id")
            menu_item.set_action_and_target_value("win.mark-as-played-by-id", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if item.TYPE in ['episode', 'season', 'show', 'movie'] and (item.isWatched or not item.isWatched and hasattr(item, 'viewOffset') and item.viewOffset != 0):
            menu_item = Gio.MenuItem.new(_("Mark as Unplayed"), "win.mark-as-unplayed-by-id")
            menu_item.set_action_and_target_value("win.mark-as-unplayed-by-id", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if self._plex.get_item_download_path(self._item) == None:
            menu_item = Gio.MenuItem.new(_("Sync"), "win.sync-by-id")
            menu_item.set_action_and_target_value("win.sync-by-id", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if item.TYPE in ['album', 'artist', 'show', 'playlist']:
            menu_item = Gio.MenuItem.new(_("Shuffle"), "win.shuffle-by-id")
            menu_item.set_action_and_target_value("win.shuffle-by-id", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if item.TYPE in ['episode', 'movie'] and hasattr(item, 'viewOffset') and item.viewOffset != 0:
            menu_item = Gio.MenuItem.new(_("Start from the beginning"), "win.play-item-from-beginning")
            menu_item.set_action_and_target_value("win.play-item-from-beginning", GLib.Variant.new_int64(self._item.ratingKey))
            menu.append_item(menu_item)

        if ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0):
            self._progress_bar.set_fraction(item.viewOffset / item.duration)
            self._progress_bar.set_visible(True)
        else:
            self._progress_bar.set_visible(False)

        self._image_height = 300
        self._image_width = self._cover_width

        if (item.TYPE == 'episode'):
            title = item.grandparentTitle
            subtitle = item.seasonEpisode + ' - ' + item.title
            if (self._show_view):
                self._image_height = 110
        elif (item.TYPE == 'movie'):
            title = item.title
            subtitle = str(item.year)
        elif (item.TYPE == 'show'):
            title = item.title
            subtitle = str(item.year)
        elif (item.TYPE == 'season'):
            title = item.parentTitle
            subtitle = item.title
        elif (item.TYPE == 'album'):
            title = item.parentTitle
            subtitle = item.title
            self._image_height = 200
        elif (item.TYPE == 'artist'):
            title = item.title
            subtitle = None
            self._image_height = 200
        elif (item.TYPE == 'playlist'):
            title = item.title
            subtitle = None
            self._image_height = 200

        if (item.TYPE == 'playlist' or item.TYPE == 'album' or item.TYPE == 'artist'):
            self._watched_image.set_visible(False)
        elif (not item.isWatched and ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0)):
            self._watched_image.set_visible(True)
        elif (item.isWatched and ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0)):
            self._watched_image.set_visible(False)
        elif (not item.isWatched):
            self._watched_image.set_visible(True)
        else:
            self._watched_image.set_visible(False)

        self._title_label.set_text(title)
        self._title_label.set_tooltip_text(title)
        if (subtitle != None):
            self._subtitle_label.set_text(subtitle)
            self._subtitle_label.set_tooltip_text(subtitle)
            self._subtitle_label.set_visible(True)
        else:
            self._subtitle_label.set_visible(False)

        #self._cover_image.set_pixel_size(self._image_width)
        self._cover_image.set_size_request(self._image_width, self._image_height)

        self.popover_menu.set_menu_model(menu)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            GLib.idle_add(self.__set_image, path)

    def __set_image(self, pix):
        self._cover_image.set_filename(pix)

    @Gtk.Template.Callback()
    def on_long_press(self, widget, x, y):
        widget.set_state(Gtk.EventSequenceState.CLAIMED);
        r = Gdk.Rectangle()
        r.x, r.y, r.width, r.height = (x, y, 10, 10)
        self.popover_menu.set_pointing_to(r)
        self.popover_menu.popup()

    @Gtk.Template.Callback()
    def on_right_click(self, widget, n_press, x, y):
        widget.set_state(Gtk.EventSequenceState.CLAIMED)
        r = Gdk.Rectangle()
        r.x, r.y, r.width, r.height = (x, y, 10, 10)
        self.popover_menu.set_pointing_to(r)
        self.popover_menu.popup()

    def __on_item_retrieved(self, plex, item):
        if (self._item.ratingKey == item.ratingKey):
            self._plex.disconnect(self._plex_retrieved_id)
            self._item = item
            self.__set_item(self._item)
            

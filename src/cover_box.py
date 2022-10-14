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


from .sync_settings import SyncSettings

import cairo
import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/cover_box.ui')
class CoverBox(Gtk.Box):
    __gtype_name__ = 'cover_box'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-album-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _title_label = Gtk.Template.Child()
    _subtitle_label = Gtk.Template.Child()
    _progress_bar = Gtk.Template.Child()
    _watched_image = Gtk.Template.Child()
    _cover_image = Gtk.Template.Child()
    _play_button = Gtk.Template.Child()
    _shuffle_button = Gtk.Template.Child()

    _menu_button = Gtk.Template.Child()
    _show_view_button = Gtk.Template.Child()
    _album_view_button = Gtk.Template.Child()
    _artist_view_button = Gtk.Template.Child()
    _mark_played_button = Gtk.Template.Child()
    _mark_unplayed_button = Gtk.Template.Child()
    _play_from_beginning_button = Gtk.Template.Child()
    _download_button = Gtk.Template.Child()

    _download_key = None
    _download_thumb = None

    def __init__(self, plex, show_view=False, cover_width=200, **kwargs):
        super().__init__(**kwargs)

        self._plex = plex
        self._cover_width = cover_width
        self._show_view = show_view
        self._plex_connect_id = self._plex.connect("download-cover", self.__on_cover_downloaded)
        self._plex_retrieved_id = self._plex.connect("item-retrieved", self.__on_item_retrieved)
        self._plex.connect("item-downloading", self.__on_item_downloading)
        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)

        self._show_view_button.connect("clicked", self.__on_go_to_show_clicked)
        self._album_view_button.connect("clicked", self.__on_go_to_album_clicked)
        self._artist_view_button.connect("clicked", self.__on_go_to_artist_clicked)
        self._mark_played_button.connect("clicked", self.__on_mark_played_clicked)
        self._mark_unplayed_button.connect("clicked", self.__on_mark_unplayed_clicked)
        self._play_from_beginning_button.connect("clicked", self.__on_play_from_beginning_clicked)
        self._download_button.connect("clicked", self.__on_download_button)

    def set_item(self, item):
        self._item = item

        self.__set_item(self._item)

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
        if ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0):
            self._progress_bar.set_fraction(item.viewOffset / item.duration)
            self._progress_bar.set_visible(True)
        else:
            self._progress_bar.set_visible(False)

        self._image_height = 300
        self._image_width = self._cover_width

        self._album_view_button.set_visible(False)
        self._artist_view_button.set_visible(False)

        if (item.TYPE == 'episode'):
            title = item.grandparentTitle
            subtitle = item.seasonEpisode + ' - ' + item.title
            self._shuffle_button.set_visible(False)
            if (self._show_view):
                self._show_view_button.set_visible(False)
                self._image_height = 110
            else:
                self._show_view_button.set_visible(True)
        elif (item.TYPE == 'movie'):
            title = item.title
            subtitle = str(item.year)
            self._shuffle_button.set_visible(False)
            self._show_view_button.set_visible(False)
        elif (item.TYPE == 'show'):
            title = item.title
            subtitle = str(item.year)
            self._shuffle_button.set_visible(True)
            self._show_view_button.set_visible(True)
        elif (item.TYPE == 'season'):
            title = item.parentTitle
            subtitle = item.title
            self._shuffle_button.set_visible(False)
            self._show_view_button.set_visible(True)
        elif (item.TYPE == 'album'):
            title = item.parentTitle
            subtitle = item.title
            self._shuffle_button.set_visible(True)
            self._show_view_button.set_visible(False)
            self._album_view_button.set_visible(True)
            self._artist_view_button.set_visible(True)
            self._image_height = 200
        elif (item.TYPE == 'artist'):
            title = item.title
            subtitle = None
            self._shuffle_button.set_visible(True)
            self._show_view_button.set_visible(False)
            self._artist_view_button.set_visible(True)
            self._image_height = 200
        elif (item.TYPE == 'playlist'):
            title = item.title
            subtitle = None
            self._shuffle_button.set_visible(True)
            self._show_view_button.set_visible(False)
            self._image_height = 200

        self._play_from_beginning_button.set_visible(False)
        if (item.TYPE == 'playlist' or item.TYPE == 'album' or item.TYPE == 'artist'):
            self._watched_image.set_visible(False)
            self._mark_unplayed_button.set_visible(False)
            self._mark_played_button.set_visible(False)
        elif (not item.isWatched and ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0)):
            self._watched_image.set_visible(True)
            self._mark_unplayed_button.set_visible(True)
            self._mark_played_button.set_visible(True)
            self._play_from_beginning_button.set_visible(True)
        elif (item.isWatched and ((item.TYPE == 'movie' or item.TYPE == 'episode') and item.viewOffset != 0)):
            self._watched_image.set_visible(False)
            self._mark_unplayed_button.set_visible(True)
            self._mark_played_button.set_visible(True)
            self._play_from_beginning_button.set_visible(True)
        elif (not item.isWatched):
            self._watched_image.set_visible(True)
            self._mark_unplayed_button.set_visible(False)
            self._mark_played_button.set_visible(True)
        else:
            self._watched_image.set_visible(False)
            self._mark_played_button.set_visible(False)
            self._mark_unplayed_button.set_visible(True)

        if (item.TYPE != 'movie' and item.TYPE != 'episode' and item.TYPE != 'album' and item.TYPE != 'playlist' and item.TYPE != 'artist' and item.TYPE != 'show'):
            self._download_button.set_visible(False)
        elif (self._plex.get_item_download_path(self._item) != None):
            self._download_button.set_visible(False)
        else:
            self._download_button.set_visible(True)

        self._title_label.set_text(title)
        self._title_label.set_tooltip_text(title)
        if (subtitle != None):
            self._subtitle_label.set_text(subtitle)
            self._subtitle_label.set_tooltip_text(subtitle)
            self._subtitle_label.set_visible(True)
        else:
            self._subtitle_label.set_visible(False)

        #self._cover_image.set_size_request(self._image_width, -1)
        self._cover_image.set_pixel_size(self._image_width)

    def __on_cover_downloaded(self, plex, rating_key, path):
        if(self._download_key == rating_key):
            #self._plex.disconnect(self._plex_connect_id)
            #pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, self._image_width, self._image_height)
            GLib.idle_add(self.__set_image, path)

    def __set_image(self, pix):
        #self._cover_image.set_from_pixbuf(pix)
        #print(pix)
        self._cover_image.set_from_file(pix)

    def __on_go_to_show_clicked(self, button):
        if self._item.TYPE == 'episode':
            self.emit('view-show-wanted', self._item.grandparentRatingKey)
        elif self._item.TYPE == 'season':
            self.emit('view-show-wanted', self._item.parentRatingKey)
        elif self._item.TYPE == 'show':
            self.emit('view-show-wanted', self._item.ratingKey)

    def __on_go_to_album_clicked(self, button):
        if self._item.TYPE == 'album':
            self.emit('view-album-wanted', self._item.ratingKey)

    def __on_go_to_artist_clicked(self, button):
        if self._item.TYPE == 'artist':
            self.emit('view-artist-wanted', self._item.ratingKey)
        elif self._item.TYPE == 'album':
            self.emit('view-artist-wanted', self._item.parentRatingKey)

    def __on_play_button_clicked(self, button):
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_play_from_beginning_clicked(self, button):
        self._menu_button.popdown()
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,),kwargs={'from_beginning':True})
        thread.daemon = True
        thread.start()

    def __on_download_button(self, button):
        self._menu_button.popdown()
        if (self._item.TYPE == 'show'):
            self._sync_settings = SyncSettings(self._plex, self._item)
            self._sync_settings.show()
        else:
            thread = threading.Thread(target=self._plex.add_to_sync, args=(self._item,))
            thread.daemon = True
            thread.start()

    def __on_shuffle_button_clicked(self, button):
        self._menu_button.popdown()
        thread = threading.Thread(target=self._plex.play_item, args=(self._item,),kwargs={'shuffle':1})
        thread.daemon = True
        thread.start()

    def __on_mark_played_clicked(self, button):
        self._menu_button.popdown()
        thread = threading.Thread(target=self._plex.mark_as_played, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_mark_unplayed_clicked(self, button):
        self._menu_button.popdown()
        thread = threading.Thread(target=self._plex.mark_as_unplayed, args=(self._item,))
        thread.daemon = True
        thread.start()

    def __on_item_retrieved(self, plex, item):
        if (self._item.ratingKey == item.ratingKey):
            self._plex.disconnect(self._plex_retrieved_id)
            self._item = item
            self.__set_item(self._item)
            
    def __on_item_downloading(self, plex, item, status):
        if (self._item.ratingKey == item.ratingKey):
            self._download_button.set_visible(False)

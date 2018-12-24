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

from gi.repository import Gtk, GLib, GdkPixbuf
from .gi_composites import GtkTemplate

import threading

@GtkTemplate(ui='/org/gnome/Girens/profile_dialog.ui')
class ProfileDialog(Gtk.Dialog):
    __gtype_name__ = 'profile_dialog'

    _avatar_image = GtkTemplate.Child()

    _ok_button = GtkTemplate.Child()
    _logout_button = GtkTemplate.Child()

    _username_value_label = GtkTemplate.Child()
    _email_value_label = GtkTemplate.Child()
    _subscriptionplan_value_label = GtkTemplate.Child()
    _secure_value_label = GtkTemplate.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("download-from-url", self.__on_downloaded)

        self._ok_button.connect("clicked", self.__on_ok_clicked)
        self._logout_button.connect("clicked", self.__on_logout_clicked)

        self._username_value_label.set_text(self._plex._account.username)
        self._email_value_label.set_text(self._plex._account.email)
        self._subscriptionplan_value_label.set_text(self._plex._account.subscriptionPlan)
        self._secure_value_label.set_text(str(self._plex._account.secure))

        thread = threading.Thread(target=self._plex.download_from_url, args=(self._plex._account.username, self._plex._account.thumb))
        thread.daemon = True
        thread.start()

    def __on_downloaded(self, plex, name_image, path):
        if(self._plex._account.username == name_image):
            pix = GdkPixbuf.Pixbuf.new_from_file_at_size(path, 200, 200)
            GLib.idle_add(self.__set_image, pix)

    def __set_image(self, pix):
        self._avatar_image.set_from_pixbuf(pix)

    def __on_ok_clicked(self, button):
        self.destroy()

    def __on_logout_clicked(self, button):
        self._plex.logout()
        self.destroy()

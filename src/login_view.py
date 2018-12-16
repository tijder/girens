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

from gi.repository import Gtk
from .gi_composites import GtkTemplate

@GtkTemplate(ui='/org/gnome/Plex/login_view.ui')
class LoginView(Gtk.Box):
    __gtype_name__ = 'login_view'

    _username_entry = GtkTemplate.Child()
    _password_entry = GtkTemplate.Child()
    _login_button = GtkTemplate.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._password_entry.connect("changed", self.__entry_changed)
        self._username_entry.connect("changed", self.__entry_changed)
        self._login_button.connect("clicked", self.__on_login_clicked)



    def __entry_changed(self, entry):
        self._password_entry.set_icon_from_icon_name(Gtk.EntryIconPosition(1), None)
        print(entry.get_text())

    def __on_login_clicked(self, button):
        self.__show_incorrect_login()

    def __show_incorrect_login(self):
        self._password_entry.set_icon_from_icon_name(Gtk.EntryIconPosition(1), 'dialog-error')

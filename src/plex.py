import time
import os.path
import urllib

from gettext import gettext as _
from plexapi.myplex import MyPlexAccount
from plexapi.server import PlexServer
from plexapi import utils
from plexapi.playqueue import PlayQueue
from gi.repository import GObject, GLib, Secret, Gio

import json

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SignalFlags.RUN_FIRST, None, (bool,str)),
        'shows-latest': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'shows-deck': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'section-shows-deck': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'download-cover': (GObject.SignalFlags.RUN_FIRST, None, (int,str)),
        'download-from-url': (GObject.SignalFlags.RUN_FIRST, None, (str,str)),
        'shows-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'item-downloading': (GObject.SignalFlags.RUN_FIRST, None, (object,bool)),
        'sync-status': (GObject.SignalFlags.RUN_FIRST, None, (bool,)),
        'servers-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'sections-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'album-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'artist-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'playlists-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'section-item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'search-item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (str,object)),
        'connection-to-server': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'logout': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'loading': (GObject.SignalFlags.RUN_FIRST, None, (str,bool)),
        'sync-items': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
    }

    _config = {}
    _search_provider_data = {}

    _server = None
    _account = None
    _library = None
    _sync_busy = False

    def __init__(self, config_dir, data_dir, player, **kwargs):
        super().__init__(**kwargs)
        self._settings = Gio.Settings ("nl.g4d.Girens")
        self._config_dir = config_dir
        self._data_dir = data_dir
        self._player = player
        self._player.set_plex(self)
        self._config = self.__open_file(self._config_dir + '/config')
        self._search_provider_data = self.__open_file(self._config_dir + '/search_provider_data')
        self._user_uuid = self._settings.get_string("user-uuid")
        self._token = self.get_token(self._user_uuid)
        self._server_uuid = self._settings.get_string("server-uuid")
        if (self._server_uuid != ''):
            self._server_token = self.get_server_token(self._server_uuid)
            self._server_url = self._settings.get_string("server-url")
        else:
            self._server_token = None
            self._server_url = None


    def __open_file(self, file_path):
        if (os.path.isfile(file_path)):
            with open(file_path, 'r') as file:
                lines = file.readlines()
                return json.loads(lines[0])
        return {}

    def has_token(self):
        return self._token is not None

    def get_server_token(self, uuid):
        try:
            return Secret.password_lookup_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': uuid}, None)
        except GLib.GError as e:
            if 'tokens' in self._config:
                return self._config['tokens'][uuid];

    def get_token(self, uuid):
        try:
            return Secret.password_lookup_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': uuid}, None)
        except GLib.GError as e:
            if 'tokens' in self._config:
                return self._config['tokens'][uuid];g

    def set_server_token(self, token, server_url, server_uuid, name):
        self._settings.set_string("server-url", self._server._baseurl)
        self._settings.set_string("server-uuid", self._server.machineIdentifier)
        try:
            Secret.password_store(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'name': Secret.SchemaAttributeType.STRING, 'url': Secret.SchemaAttributeType.STRING, 'uuid': Secret.SchemaAttributeType.STRING}), {'name': name, 'url': server_url, 'uuid': server_uuid}, Secret.COLLECTION_DEFAULT, 'Girens server token', token, None, None)
            Secret.password_lookup_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': server_uuid}, None)
        except GLib.GError as e:
            if 'tokens' not in self._config:
                self._config['tokens'] = {}
            self._config['tokens'][server_uuid] = token
            self.__save_config()

    def set_token(self, token, username, email, uuid):
        self._settings.set_string("user-uuid", uuid)
        try:
            Secret.password_store(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'username': Secret.SchemaAttributeType.STRING, 'email': Secret.SchemaAttributeType.STRING, 'uuid': Secret.SchemaAttributeType.STRING}), {'username': username, 'email': email, 'uuid': uuid}, Secret.COLLECTION_DEFAULT, 'Girens token', token, None, None)
            Secret.password_lookup_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': uuid}, None)
        except GLib.GError as e:
            if 'tokens' not in self._config:
                self._config['tokens'] = {}
            self._config['tokens'][uuid] = token
            self.__save_config()

    def has_url(self):
        return self._server_url is not None

    def login_token(self, token):
        try:
            self._account = MyPlexAccount(token=token)
            self.set_token(self._account._token, self._account.username, self._account.email, self._account.uuid)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def login(self, username, password):
        try:
            self._account = MyPlexAccount(username, password)
            self.set_token(self._account._token, self._account.username, self._account.email, self._account.uuid)
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def login_with_url(self, baseurl, token):
        try:
            self.emit('loading', _('Connecting to ') + baseurl, True)
            self._server = PlexServer(baseurl, token)
            self._account = self._server.account()
            self._library = self._server.library
            self.set_server_token(self._server._token, self._server._baseurl, self._server.machineIdentifier, self._server.friendlyName)
            Secret.password_clear_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': self._user_uuid}, None)
            self._user_uuid = None
            self._token = None
            self.emit('connection-to-server')
            self.emit('loading', 'Success', False)
            self.emit('login-status',True,'')
        except:
            self.emit('loading', _('Connecting to ') + baseurl + _(' failed.'), True)
            self.emit('login-status',False,'Login failed')
            print('connection failed (login with url)')

    def __save_config(self):
        with open(self._config_dir + '/config', 'w') as file:
            file.write(json.dumps(self._config))

    def __save_search_provider_data(self):
        with open(self._config_dir + '/search_provider_data', 'w') as file:
            file.write(json.dumps(self._search_provider_data))

    def logout(self):
        self._config = {}
        self._server = None
        self._account = None
        self._library = None
        self.__remove_login()
        self.emit('logout')

    def __remove_login(self):
        if (os.path.isfile(self._config_dir + '/config')):
            os.remove(self._config_dir + '/config')
        Secret.password_clear_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': self._server_uuid}, None)
        Secret.password_clear_sync(Secret.Schema.new("nl.g4d.Girens", Secret.SchemaFlags.NONE, {'uuid': Secret.SchemaAttributeType.STRING}), {'uuid': self._user_uuid}, None)
        self._settings.set_string("server-url", '')
        self._settings.set_string("server-uuid", '')
        self._settings.set_string("user-uuid", '')

        self._user_uuid = None
        self._token = None
        self._server_uuid = None
        self._server_token = None
        self._server_url = None

    def get_latest(self):
        latest = self._library.recentlyAdded()
        self.emit('shows-latest',latest)

    def get_deck(self):
        deck = self._library.onDeck()
        self.emit('shows-deck',deck)

    def get_section_deck(self, section_id):
        deck = self._library.sectionByID(section_id).onDeck()
        self.emit('section-shows-deck',deck)

    def get_item(self, key):
        return self._server.fetchItem(int(key))

    def get_show(self, key):
        show = self._server.fetchItem(int(key))
        episodes = show.episodes()
        self.emit('shows-retrieved',show, episodes)

    def get_album(self, key):
        album = self._server.fetchItem(int(key))
        tracks = album.tracks()
        self.emit('album-retrieved', album, tracks)

    def get_artist(self, key):
        artist = self._server.fetchItem(int(key))
        albums = artist.albums()
        self.emit('artist-retrieved', artist, albums)

    def get_servers(self):
        servers = []
        if (self.has_token()):
            for resource in self._account.resources():
                if (resource.provides == 'server'):
                    servers.append(resource)
        else:
            servers.append(self._server)
        self.emit('servers-retrieved', servers)

    def get_playlists(self):
        playlists = self._server.playlists()
        self.emit('playlists-retrieved', playlists)

    def get_sections(self):
        sections = self._library.sections()
        self.emit('sections-retrieved', sections)

    def get_section_filter(self, section):
        if ('sections' in self._config and section.uuid in self._config['sections'] and 'sort' in self._config['sections'][section.uuid] ):
            return self._config['sections'][section.uuid]
        return None

    def get_section_items(self, section, container_start=0, container_size=10, sort=None, sort_value=None):
        if (sort != None):
            if 'sections' not in self._config:
                self._config['sections'] = {}
            if section.uuid not in self._config['sections']:
                self._config['sections'][section.uuid] = {}
            self._config['sections'][section.uuid]['sort'] = sort
            self._config['sections'][section.uuid]['sort_value'] = sort_value
            self.__save_config()
            sort = sort + ':' + sort_value
        items = section.all(container_start=container_start, container_size=container_size, sort=sort)
        self.emit('section-item-retrieved', items)

    def reload_search_provider_data(self):
        #section = self._library.sectionByID('22')
        self._search_provider_data['sections'] = {}
        for section in self._library.sections():
            if (section.type not in  ['photo', 'movie']):
                items = section.all()
                self._search_provider_data['sections'][section.uuid] = {
                    'key': section.key,
                    'server_machine_identifier': self._server.machineIdentifier,
                    'title': section.title
                }
                self._search_provider_data['sections'][section.uuid]['items'] = []
                for item in items:
                    self._search_provider_data['sections'][section.uuid]['items'].append({
                        'title': item.title,
                        'titleSort': item.titleSort,
                        'ratingKey': item.ratingKey,
                        'type': item.type
                    })
                if (section.type == 'artist'):
                    for item in section.albums():
                        self._search_provider_data['sections'][section.uuid]['items'].append({
                        'title': item.title,
                        'titleSort': item.titleSort,
                        'ratingKey': item.ratingKey,
                        'type': item.type
                    })
        self.__save_search_provider_data()

    def search_library(self, search, libtype=None):
        items = self._library.search(search, limit=10, libtype=libtype)
        self.emit('search-item-retrieved', search, items)

    def download_cover(self, key, thumb):
        url_image = self._server.transcodeImage(thumb, 300, 200)
        if (url_image is not None and url_image != ""):
            path = self.__download(url_image, 'thumb_' + str(key))
            self.emit('download-cover', key, path)

    def download_from_url(self, name_image, url_image):
        if (url_image is not None and url_image != ""):
            path = self.__download(url_image, 'thumb_' + name_image)
            self.emit('download-from-url', name_image, path)

    def play_item(self, item, shuffle=0, from_beginning=None, sort=None):
        if type(item) is str:
            item = self._server.fetchItem(item)
        parent_item = None
        if item.TYPE == "track":
            parent_item = item.album()
        playqueue = PlayQueue.create(self._server, item, shuffle=shuffle, continuous=1, parent=parent_item, sort=sort)
        self._player.set_playqueue(playqueue)
        GLib.idle_add(self.__play_item, from_beginning)

    def __play_item(self, from_beginning):
        self._player.start(from_beginning=from_beginning)

    def get_sync_items(self):
        if 'sync' in self._config:
            self.emit('sync-items', self._config['sync'])

    def remove_from_sync(self, item_key):
        if str(item_key) in self._config['sync']:
            del self._config['sync'][item_key]
            self.__save_config()
            self.get_sync_items()

    def add_to_sync(self, item, converted=False, max_items=None, only_unwatched=False):
        if 'sync' not in self._config:
            self._config['sync'] = {}
        if str(item.ratingKey) not in self._config['sync']:
            self._config['sync'][str(item.ratingKey)] = {}
            self._config['sync'][str(item.ratingKey)]['rating_key'] = str(item.ratingKey)
            self._config['sync'][str(item.ratingKey)]['converted'] = converted
            self._config['sync'][str(item.ratingKey)]['only_unwatched'] = only_unwatched
            if (max_items != None):
                self._config['sync'][str(item.ratingKey)]['max_items'] = max_items
            self.__save_config()
            self.get_sync_items()
        self.sync()

    def sync(self):
        if (self._sync_busy == False):
            self.emit('sync-status', True)
            path_dir = self._data_dir + '/' + self._server.machineIdentifier
            download_files = []
            for file in os.listdir(path_dir):
                if file.startswith("item_"):
                    download_files.append(file)
            self._sync_busy = True
            if 'sync' in self._config:
                sync = self._config['sync'].copy()
                for item_keys in sync:
                    item = self._server.fetchItem(int(item_keys))

                    download_items = []
                    if (item.TYPE == 'movie' or item.TYPE == 'episode'):
                        download_items.append(item)
                    elif (item.TYPE == 'album' or item.TYPE == 'artist'):
                        download_items = item.tracks()
                    elif (item.TYPE == 'playlist'):
                        download_items = item.items()
                    elif (item.TYPE == 'show'):
                        download_items = item.episodes()
                    count = 0
                    for download_item in download_items:
                        sync_bool = False
                        if ('only_unwatched' not in sync[item_keys]):
                            sync_bool = True
                        elif (sync[item_keys]['only_unwatched'] == False):
                            sync_bool = True
                        elif (sync[item_keys]['only_unwatched'] == True and (download_item.TYPE == 'movie' or download_item.TYPE == 'episode') and not download_item.isWatched):
                            sync_bool = True

                        if (sync_bool == True):
                            count = count + 1
                            if ('max_items' in sync[item_keys] and count > int(sync[item_keys]['max_items'])):
                                break
                            if(self.get_item_download_path(download_item) == None):
                                self.__download_item(download_item, converted=sync[item_keys]['converted'])
                            if ('item_' + str(download_item.ratingKey) in download_files):
                                download_files.remove('item_' + str(download_item.ratingKey))
            for file in download_files:
                path_file = os.path.join(path_dir, file)
                if os.path.exists(path_file):
                    os.remove(path_file)
            self.emit('sync-status', False)
            self._sync_busy = False

    def __download_item(self, item, converted=False):
        path_dir = self._data_dir + '/' + self._server.machineIdentifier
        filename = 'item_' + str(item.ratingKey)
        filename_tmp = filename + '.tmp'
        path = path_dir + '/' + filename
        path_tmp = path_dir + '/' + filename_tmp

        self.emit('item-downloading', item, True)

        if not os.path.exists(path_dir):
            os.makedirs(path_dir)
        if not os.path.exists(path):
            if os.path.exists(path_tmp):
                os.remove(path_tmp)
            locations = [i for i in item.iterParts() if i]
            if (converted == False):
                download_url = self._server.url('%s?download=1' % locations[0].key)
            else:
                download_url = item.getStreamURL()
            utils.download(download_url, self._server._token, filename=filename_tmp, savepath=path_dir, session=self._server._session)
            os.rename(path_tmp, path)
            self.emit('item-downloading', item, False)

    def get_item_download_path(self, item):
        path_dir = self._data_dir + '/' + self._server.machineIdentifier
        filename = 'item_' + str(item.ratingKey)
        path = path_dir + '/' + filename
        if not os.path.exists(path):
            return None
        return path

    def mark_as_played(self, item):
        item.markWatched()
        item.reload()
        self.emit('item-retrieved', item)

    def mark_as_unplayed(self, item):
        item.markUnwatched()
        item.reload()
        self.emit('item-retrieved', item)

    def retrieve_item(self, item_key):
        item = self._server.fetchItem(int(item_key))
        self.emit('item-retrieved', item)

    def path_for_download(self, prefix):
        path_dir = self._data_dir + '/' + self._server.machineIdentifier
        path = path_dir + '/' + prefix
        return [path_dir, path]

    def __download(self, url_image, prefix):
        paths = self.path_for_download(prefix)
        path_dir = paths[0]
        path = paths[1]


        if not os.path.exists(path_dir):
            os.makedirs(path_dir)
        if not os.path.exists(path):
            parse = urllib.parse.urlparse(url_image)
            auth_user = parse.username
            auth_passwd = parse.password
            password_mgr = urllib.request.HTTPPasswordMgrWithDefaultRealm()
            password_mgr.add_password(None, parse.scheme + "://" + parse.hostname, auth_user, auth_passwd)
            handler = urllib.request.HTTPBasicAuthHandler(password_mgr)
            opener = urllib.request.build_opener(handler)
            port = ""
            if parse.port != None:
                port = ":" + str(parse.port)
            url_img_combined = parse.scheme + "://" + parse.hostname + port + parse.path + "?" + parse.query
            img_raw = opener.open(url_img_combined)
            with open(path, 'w+b') as file:
                file.write(img_raw.read())
            return path
        else:
            return path

    def connect_to_server(self):
        if (self._server_token is not None and self._server_url is not None):
            try:
                self.emit('loading', _('Connecting to ') + self._server_url + '.', True)
                self._server = PlexServer(self._server_url, self._server_token)
                self._library = self._server.library
                self.set_server_token(self._server._token, self._server._baseurl, self._server.machineIdentifier, self._server.friendlyName)
                self.emit('connection-to-server')
                self.emit('loading', 'Success', False)
                return None
            except:
                self.emit('loading', _('Connecting to ') + self._server_url + _(' failed.'), True)
                print('custom url connection failed')

        servers_found = False
        for resource in self._account.resources():
            servers_found = True
            if ('server' in resource.provides.split(',')):
                if self.connect_to_resource(resource):
                    break

        if (servers_found == False):
            self.emit('loading', _('No servers found for this account.'), True)

    def connect_to_resource(self, resource):
        try:
            self.emit('loading', _('Connecting to ') + resource.name + '.\n'+ _('There are ') + str(len(resource.connections)) + _(' connection urls.') + '\n' + _('This may take a while'), True)
            self._server = resource.connect(ssl=self._account.secure)
            self._library = self._server.library
            self.set_server_token(self._server._token, self._server._baseurl, self._server.machineIdentifier, self._server.friendlyName)
            self.emit('connection-to-server')
            self.emit('loading', 'Success', False)
            return True
        except:
            self.emit('loading', _('Connecting to ') + resource.name + _(' failed.'), True)
            print('connection failed (when trying to connect to resource)')
            return False

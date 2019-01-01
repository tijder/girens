import time
import os.path
import urllib

from plexapi.myplex import MyPlexAccount
from plexapi.server import PlexServer
from plexapi import utils
from plexapi.playqueue import PlayQueue
from gi.repository import GObject

import json

class Plex(GObject.Object):
    __gsignals__ = {
        'login-status': (GObject.SignalFlags.RUN_FIRST, None, (bool,str)),
        'shows-latest': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'shows-deck': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'download-cover': (GObject.SignalFlags.RUN_FIRST, None, (int,str)),
        'download-from-url': (GObject.SignalFlags.RUN_FIRST, None, (str,str)),
        'shows-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,object)),
        'item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'item-downloading': (GObject.SignalFlags.RUN_FIRST, None, (object,bool)),
        'servers-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'sections-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'playlists-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'section-item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (object,)),
        'search-item-retrieved': (GObject.SignalFlags.RUN_FIRST, None, (str,object)),
        'connection-to-server': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'logout': (GObject.SignalFlags.RUN_FIRST, None, ()),
        'loading': (GObject.SignalFlags.RUN_FIRST, None, (str,bool)),
    }

    _config = {}

    _server = None
    _account = None
    _library = None

    def __init__(self, config_dir, data_dir, player, **kwargs):
        super().__init__(**kwargs)
        self._config_dir = config_dir
        self._data_dir = data_dir
        self._player = player
        self._player.set_plex(self)
        if(os.path.isfile(self._config_dir + '/config')):
           with open(self._config_dir + '/config', 'r') as file:
               lines = file.readlines()
               self._config = json.loads(lines[0])

    def has_token(self):
        return 'token' in self._config

    def login_token(self, token):
        try:
            self._account = MyPlexAccount(token=token)
            self._config['token'] = self._account._token
            self.__save_config()
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def login(self, username, password):
        try:
            self._account = MyPlexAccount(username, password)
            self._config['token'] = self._account._token
            self.__save_config()
            self.emit('login-status',True,'')
        except:
            self.emit('login-status',False,'Login failed')

    def __save_config(self):
        with open(self._config_dir + '/config', 'w') as file:
            file.write(json.dumps(self._config))

    def logout(self):
        self._config = {}
        self._server = None
        self._account = None
        self._library = None
        self.__remove_login()
        self.emit('logout')

    def __remove_login(self):
        os.remove(self._config_dir + '/config')

    def get_latest(self):
        latest = self._library.recentlyAdded()
        self.emit('shows-latest',latest)

    def get_deck(self):
        deck = self._library.onDeck()
        self.emit('shows-deck',deck)

    def get_show(self, key):
        show = self._server.fetchItem(int(key))
        episodes = show.episodes()
        self.emit('shows-retrieved',show, episodes)

    def get_servers(self):
        servers = []
        for resource in self._account.resources():
            if (resource.provides == 'server'):
                servers.append(resource)
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

    def get_section_items(self, section, sort=None, sort_value=None):
        if (sort != None):
            if 'sections' not in self._config:
                self._config['sections'] = {}
            if section.uuid not in self._config['sections']:
                self._config['sections'][section.uuid] = {}
            self._config['sections'][section.uuid]['sort'] = sort
            self._config['sections'][section.uuid]['sort_value'] = sort_value
            self.__save_config()
            sort = sort + ':' + sort_value
        items = section.all(sort=sort)
        self.emit('section-item-retrieved', items)

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

    def play_item(self, item, shuffle=0, from_beginning=False):
        playqueue = PlayQueue.create(self._server, item, shuffle=shuffle)
        self._player.set_playqueue(playqueue)
        self._player.start(from_beginning=from_beginning)

    def download_item(self, item):
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
            download_url = self._server.url('%s?download=1' % locations[0].key)
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

    def __download(self, url_image, prefix):
        path_dir = self._data_dir + '/' + self._server.machineIdentifier
        path = path_dir + '/' + prefix

        if not os.path.exists(path_dir):
            os.makedirs(path_dir)
        if not os.path.exists(path):
            urllib.request.urlretrieve(url_image, path)
            return path
        else:
            return path

    def connect_to_server(self):
        if ('server_url' in self._config and 'server_token' in self._config):
            try:
                self.emit('loading', 'Connecting to ' + self._config['server_url'] + '.', True)
                self._server = PlexServer(self._config['server_url'], self._config['server_token'])
                self._library = self._server.library
                self.emit('connection-to-server')
                self.emit('loading', 'Success', False)
                return None
            except:
                self.emit('loading', 'Connecting to ' + resource.name + ' failed.', True)
                print('custom url connection failed')

        servers_found = False
        for resource in self._account.resources():
            servers_found = True
            if ('server' in resource.provides.split(',')):
                try:
                    self.emit('loading', 'Connecting to ' + resource.name + '.\nThere are ' + str(len(resource.connections)) + ' connection urls.\nThis may take a while', True)
                    self._server = resource.connect(ssl=self._account.secure)
                    self._library = self._server.library
                    self._config['server_url'] = self._server._baseurl
                    self._config['server_token'] = self._server._token
                    self.__save_config()
                    self.emit('connection-to-server')
                    self.emit('loading', 'Success', False)
                    break
                except:
                    self.emit('loading', 'Connecting to ' + resource.name + ' failed.', True)
                    print('connection failed')
        if (servers_found == False):
            self.emit('loading', 'No servers found for this account.', True)

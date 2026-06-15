import 'dart:async';

import 'package:flutter/material.dart';

import 'api.dart';
import 'controller.dart';
import 'theme.dart';
import 'widgets.dart';

void pushScreen(BuildContext context, Widget screen) {
  Navigator.of(context).push(wavePageRoute(screen));
}

PageRoute<void> wavePageRoute(Widget screen) {
  return PageRouteBuilder<void>(
    pageBuilder: (_, _, _) => PageSurface(child: screen),
    transitionDuration: const Duration(milliseconds: 120),
    reverseTransitionDuration: const Duration(milliseconds: 90),
    transitionsBuilder: (_, animation, _, child) {
      return FadeTransition(
        opacity: CurvedAnimation(parent: animation, curve: Curves.easeOut),
        child: child,
      );
    },
  );
}

int responsiveColumns(BuildContext context, {double tile = 180}) {
  final width = MediaQuery.of(context).size.width;
  // The content area is bounded; use up to ~900 for the grid math on desktop.
  final usable = (width > kDesktopBreakpoint ? width - 260 : width).clamp(
    280.0,
    1100.0,
  );
  return (usable / tile).floor().clamp(2, 6);
}

/// A page padded for the content column; wider gutters on desktop.
class ContentPadding extends StatelessWidget {
  const ContentPadding({required this.child, super.key});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [child],
    );
  }
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

class HomeScreen extends StatefulWidget {
  const HomeScreen({
    required this.controller,
    required this.username,
    required this.onLogout,
    super.key,
  });

  final WaveController controller;
  final String username;
  final VoidCallback onLogout;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  late Future<StarredAll> _liked;
  late Future<List<Album>> _recent;

  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _liked = api.getStarred();
    _recent = api
        .getAlbumList('newest')
        .then((albums) => api.repairAlbumCovers(albums, limit: 10));
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Музыка',
                style: serif(fontSize: 34, fontWeight: FontWeight.w700),
              ),
            ),
            _AccountPill(username: widget.username, onTap: widget.onLogout),
          ],
        ),
        const SizedBox(height: 20),
        ListenableBuilder(
          listenable: widget.controller,
          builder: (context, _) => WaveHero(
            loading: widget.controller.loadingWave,
            ready: widget.controller.waveReady,
            onPlay: widget.controller.startWave,
          ),
        ),
        ListenableBuilder(
          listenable: widget.controller,
          builder: (context, _) {
            final error = widget.controller.error;
            if (error == null) return const SizedBox.shrink();
            return Padding(
              padding: const EdgeInsets.only(top: 14),
              child: Text(
                error,
                style: const TextStyle(color: Colors.redAccent),
              ),
            );
          },
        ),
        const SizedBox(height: 28),
        FutureBuilder<StarredAll>(
          future: _liked,
          builder: (context, snap) {
            final songs = snap.data?.songs ?? const <Song>[];
            if (songs.isEmpty) return const SizedBox.shrink();
            return _LikedTracksPager(
              controller: widget.controller,
              songs: songs,
            );
          },
        ),
        FutureBuilder<List<Album>>(
          future: _recent,
          builder: (context, snap) {
            final albums = snap.data ?? const <Album>[];
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SectionTitle(
                  'Недавнее',
                  trailing: TextButton(
                    onPressed: () => pushScreen(
                      context,
                      AlbumsScreen(controller: widget.controller),
                    ),
                    child: const Text(
                      'Всё',
                      style: TextStyle(
                        color: kPink,
                        fontWeight: FontWeight.w800,
                      ),
                    ),
                  ),
                ),
                if (snap.connectionState != ConnectionState.done)
                  const StatusView(loading: true, error: null)
                else
                  SizedBox(
                    height: 188,
                    child: ListView.separated(
                      scrollDirection: Axis.horizontal,
                      itemCount: albums.length,
                      separatorBuilder: (_, _) => const SizedBox(width: 14),
                      itemBuilder: (context, i) => AlbumCard(
                        api: api,
                        album: albums[i],
                        width: 140,
                        onTap: () => pushScreen(
                          context,
                          AlbumScreen(
                            controller: widget.controller,
                            albumId: albums[i].id,
                            title: albums[i].name,
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _AccountPill extends StatelessWidget {
  const _AccountPill({required this.username, required this.onTap});
  final String username;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return InkSurface(
      child: InkWell(
        borderRadius: BorderRadius.circular(999),
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.fromLTRB(6, 6, 14, 6),
          decoration: BoxDecoration(
            color: Colors.white.withValues(alpha: 0.05),
            borderRadius: BorderRadius.circular(999),
            border: Border.all(color: Colors.white.withValues(alpha: 0.10)),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 28,
                height: 28,
                alignment: Alignment.center,
                decoration: const BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: LinearGradient(
                    colors: [Color(0xffa4243b), kViolet],
                  ),
                ),
                child: Text(
                  username.isEmpty
                      ? '?'
                      : username.substring(0, 1).toUpperCase(),
                  style: const TextStyle(
                    fontWeight: FontWeight.w900,
                    fontSize: 13,
                    color: kCream,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Text(
                username,
                style: const TextStyle(
                  fontWeight: FontWeight.w700,
                  color: Colors.white70,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class WaveHero extends StatefulWidget {
  const WaveHero({
    required this.onPlay,
    required this.loading,
    required this.ready,
    super.key,
  });
  final VoidCallback onPlay;
  final bool loading;
  final bool ready;

  @override
  State<WaveHero> createState() => _WaveHeroState();
}

class _WaveHeroState extends State<WaveHero>
    with SingleTickerProviderStateMixin {
  late final AnimationController _flow = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 16),
  )..repeat(reverse: true);

  static const _colors = [
    Color(0xff16060c),
    Color(0xff7a0c1f),
    Color(0xffa4243b),
    Color(0xff45122e),
    Color(0xff0c0410),
  ];
  static const _stops = [0.0, 0.35, 0.5, 0.72, 1.0];

  @override
  void dispose() {
    _flow.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _flow,
      builder: (context, child) {
        // Sweep the gradient axis back and forth — the flowing "wave-pan".
        final t = Curves.easeInOut.transform(_flow.value);
        final begin = Alignment(-1.0 + 2 * t, -1.0);
        final end = Alignment(1.0 - 2 * t, 1.0);
        return DecoratedBox(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(16),
            gradient: LinearGradient(
              begin: begin,
              end: end,
              colors: _colors,
              stops: _stops,
            ),
            border: Border.all(color: kPink.withValues(alpha: 0.25)),
            boxShadow: [
              BoxShadow(
                color: kPink.withValues(alpha: 0.22),
                blurRadius: 34,
                offset: const Offset(0, 18),
              ),
            ],
          ),
          child: child,
        );
      },
      child: ClipRRect(
        borderRadius: BorderRadius.circular(16),
        child: InkSurface(
          child: InkWell(
            onTap: widget.loading ? null : widget.onPlay,
            child: SizedBox(
              height: 210,
              child: Stack(
                children: [
                  Positioned.fill(
                    child: CustomPaint(
                      painter: GothicCrossPainter(
                        Colors.black.withValues(alpha: 0.22),
                      ),
                    ),
                  ),
                  Padding(
                    padding: const EdgeInsets.all(24),
                    child: Stack(
                      children: [
                        Align(
                          alignment: Alignment.bottomLeft,
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                'Твоя волна',
                                style: serif(
                                  fontSize: 44,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                              const SizedBox(height: 6),
                              Text(
                                widget.loading
                                    ? 'загружаем рекомендации...'
                                    : widget.ready
                                    ? 'очередь готова, можно слушать'
                                    : 'готовим очередь рекомендаций...',
                                style: const TextStyle(
                                  fontSize: 15,
                                  fontWeight: FontWeight.w700,
                                  color: Colors.white70,
                                ),
                              ),
                            ],
                          ),
                        ),
                        Align(
                          alignment: Alignment.topRight,
                          child: Container(
                            width: 60,
                            height: 60,
                            alignment: Alignment.center,
                            decoration: BoxDecoration(
                              shape: BoxShape.circle,
                              color: Colors.black.withValues(alpha: 0.55),
                              border: Border.all(
                                color: kPink.withValues(alpha: 0.4),
                              ),
                            ),
                            child: widget.loading
                                ? const SizedBox.square(
                                    dimension: 22,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 3,
                                      color: kCream,
                                    ),
                                  )
                                : Icon(
                                    widget.ready
                                        ? Icons.play_arrow_rounded
                                        : Icons.hourglass_top_rounded,
                                    color: kCream,
                                    size: 34,
                                  ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _LikedTracksPager extends StatelessWidget {
  const _LikedTracksPager({required this.controller, required this.songs});
  final WaveController controller;
  final List<Song> songs;

  @override
  Widget build(BuildContext context) {
    final capped = songs.take(24).toList();
    final pages = <List<Song>>[];
    for (var i = 0; i < capped.length; i += 4) {
      pages.add(capped.sublist(i, (i + 4).clamp(0, capped.length)));
    }
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    final pageWidth = (MediaQuery.of(context).size.width * (wide ? 0.42 : 0.85))
        .clamp(280.0, 460.0);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionTitle(
          'Любимые треки',
          trailing: TextButton(
            onPressed: () =>
                pushScreen(context, LikedScreen(controller: controller)),
            child: const Text(
              'Всё',
              style: TextStyle(color: kPink, fontWeight: FontWeight.w800),
            ),
          ),
        ),
        SizedBox(
          height: 4 * 68.0,
          child: ListView.separated(
            scrollDirection: Axis.horizontal,
            itemCount: pages.length,
            separatorBuilder: (_, _) => const SizedBox(width: 16),
            itemBuilder: (context, p) {
              final page = pages[p];
              return SizedBox(
                width: pageWidth,
                child: Column(
                  children: [
                    for (final song in page)
                      SongTile(
                        controller: controller,
                        song: song,
                        onTap: () =>
                            controller.playQueue(capped, capped.indexOf(song)),
                      ),
                  ],
                ),
              );
            },
          ),
        ),
        const SizedBox(height: 28),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

class SearchScreen extends StatefulWidget {
  const SearchScreen({
    required this.controller,
    this.initialQuery,
    this.showBack = false,
    super.key,
  });
  final WaveController controller;
  final String? initialQuery;
  final bool showBack;

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _field = TextEditingController();
  Timer? _debounce;
  Future<SearchResults>? _results;

  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    final query = widget.initialQuery?.trim();
    if (query != null && query.length >= 2) {
      _field.text = query;
      _results = api.search(query);
    }
  }

  void _onChanged(String value) {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), () {
      final query = value.trim();
      setState(() {
        _results = query.length >= 2 ? api.search(query) : null;
      });
    });
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _field.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        ScreenHeader(title: 'Поиск', showBack: widget.showBack),
        TextField(
          controller: _field,
          autofocus: true,
          onChanged: _onChanged,
          style: const TextStyle(fontWeight: FontWeight.w600),
          decoration: InputDecoration(
            hintText: 'Песни, артисты, альбомы',
            prefixIcon: const Icon(Icons.search_rounded, color: Colors.white38),
            filled: true,
            fillColor: Colors.white.withValues(alpha: 0.05),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(16),
              borderSide: BorderSide(
                color: Colors.white.withValues(alpha: 0.10),
              ),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(16),
              borderSide: const BorderSide(color: kPink, width: 2),
            ),
          ),
        ),
        const SizedBox(height: 22),
        if (_results != null)
          FutureBuilder<SearchResults>(
            future: _results,
            builder: (context, snap) {
              if (snap.connectionState != ConnectionState.done) {
                return const StatusView(loading: true, error: null);
              }
              final data = snap.data;
              if (data == null || data.isEmpty) {
                return const Padding(
                  padding: EdgeInsets.symmetric(vertical: 40),
                  child: Center(
                    child: Text(
                      'Ничего не нашлось',
                      style: TextStyle(color: Colors.white54),
                    ),
                  ),
                );
              }
              return Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (data.artists.isNotEmpty) ...[
                    const SectionTitle('Артисты'),
                    for (final artist in data.artists)
                      ArtistRow(
                        api: api,
                        artist: artist,
                        onTap: () => pushScreen(
                          context,
                          ArtistScreen(
                            controller: widget.controller,
                            artistId: artist.id,
                            title: artist.name,
                          ),
                        ),
                      ),
                    const SizedBox(height: 22),
                  ],
                  if (data.albums.isNotEmpty) ...[
                    const SectionTitle('Альбомы'),
                    SizedBox(
                      height: 188,
                      child: ListView.separated(
                        scrollDirection: Axis.horizontal,
                        itemCount: data.albums.length,
                        separatorBuilder: (_, _) => const SizedBox(width: 14),
                        itemBuilder: (context, i) => AlbumCard(
                          api: api,
                          album: data.albums[i],
                          width: 140,
                          onTap: () => pushScreen(
                            context,
                            AlbumScreen(
                              controller: widget.controller,
                              albumId: data.albums[i].id,
                              title: data.albums[i].name,
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 22),
                  ],
                  if (data.songs.isNotEmpty) ...[
                    const SectionTitle('Песни'),
                    for (final song in data.songs)
                      SongTile(
                        controller: widget.controller,
                        song: song,
                        onTap: () => widget.controller.playQueue(
                          data.songs,
                          data.songs.indexOf(song),
                        ),
                      ),
                  ],
                ],
              );
            },
          ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

class LibraryScreen extends StatefulWidget {
  const LibraryScreen({required this.controller, super.key});
  final WaveController controller;

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> {
  late Future<List<Artist>> _artists;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _artists = api.getArtists();
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        const ScreenHeader(title: 'Медиатека', showBack: false),
        Row(
          children: [
            _LibraryTile(
              icon: Icons.queue_music_rounded,
              label: 'Плейлисты',
              color: kViolet,
              onTap: () => pushScreen(
                context,
                PlaylistsScreen(controller: widget.controller),
              ),
            ),
            const SizedBox(width: 12),
            _LibraryTile(
              icon: Icons.album_rounded,
              label: 'Альбомы',
              color: const Color(0xffa4243b),
              onTap: () => pushScreen(
                context,
                AlbumsScreen(controller: widget.controller),
              ),
            ),
            const SizedBox(width: 12),
            _LibraryTile(
              icon: Icons.favorite_rounded,
              label: 'Любимое',
              color: kPink,
              onTap: () => pushScreen(
                context,
                LikedScreen(controller: widget.controller),
              ),
            ),
          ],
        ),
        const SizedBox(height: 26),
        const SectionTitle('Артисты'),
        FutureBuilder<List<Artist>>(
          future: _artists,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final artists = snap.data ?? const <Artist>[];
            return Column(
              children: [
                for (final artist in artists)
                  ArtistRow(
                    api: api,
                    artist: artist,
                    onTap: () => pushScreen(
                      context,
                      ArtistScreen(
                        controller: widget.controller,
                        artistId: artist.id,
                        title: artist.name,
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _LibraryTile extends StatelessWidget {
  const _LibraryTile({
    required this.icon,
    required this.label,
    required this.color,
    required this.onTap,
  });
  final IconData icon;
  final String label;
  final Color color;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: InkSurface(
        child: InkWell(
          borderRadius: BorderRadius.circular(16),
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(vertical: 18, horizontal: 14),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: Colors.white.withValues(alpha: 0.10)),
              gradient: LinearGradient(
                begin: Alignment.topLeft,
                end: Alignment.bottomRight,
                colors: [color.withValues(alpha: 0.18), Colors.transparent],
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(icon, color: color),
                const SizedBox(height: 10),
                Text(
                  label,
                  style: const TextStyle(
                    fontWeight: FontWeight.w800,
                    color: kCream,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Albums (grid)
// ---------------------------------------------------------------------------

class AlbumsScreen extends StatefulWidget {
  const AlbumsScreen({required this.controller, super.key});
  final WaveController controller;

  @override
  State<AlbumsScreen> createState() => _AlbumsScreenState();
}

class _AlbumsScreenState extends State<AlbumsScreen> {
  String _type = 'newest';
  late Future<List<Album>> _albums;
  SongarrApi get api => widget.controller.api;

  static const _filters = {
    'newest': 'Новые',
    'frequent': 'Частые',
    'alphabeticalByName': 'А-Я',
  };

  @override
  void initState() {
    super.initState();
    _load();
  }

  void _load() {
    _albums = api
        .getAlbumList(_type, size: 200)
        .then((albums) => api.repairAlbumCovers(albums, limit: 80));
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    final cols = responsiveColumns(context, tile: 170);
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        const ScreenHeader(title: 'Альбомы'),
        Wrap(
          spacing: 8,
          children: [
            for (final entry in _filters.entries)
              ChoiceChip(
                label: Text(entry.value),
                selected: _type == entry.key,
                onSelected: (_) => setState(() {
                  _type = entry.key;
                  _load();
                }),
                selectedColor: kPink,
                backgroundColor: Colors.white.withValues(alpha: 0.05),
                labelStyle: TextStyle(
                  fontWeight: FontWeight.w800,
                  color: _type == entry.key ? Colors.white : Colors.white60,
                ),
                side: BorderSide.none,
              ),
          ],
        ),
        const SizedBox(height: 18),
        FutureBuilder<List<Album>>(
          future: _albums,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final albums = snap.data ?? const <Album>[];
            return GridView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: cols,
                crossAxisSpacing: 16,
                mainAxisSpacing: 18,
                childAspectRatio: 0.72,
              ),
              itemCount: albums.length,
              itemBuilder: (context, i) => AlbumCard(
                api: api,
                album: albums[i],
                onTap: () => pushScreen(
                  context,
                  AlbumScreen(
                    controller: widget.controller,
                    albumId: albums[i].id,
                    title: albums[i].name,
                  ),
                ),
              ),
            );
          },
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Artist
// ---------------------------------------------------------------------------

class ArtistScreen extends StatefulWidget {
  const ArtistScreen({
    required this.controller,
    required this.artistId,
    required this.title,
    super.key,
  });
  final WaveController controller;
  final String artistId;
  final String title;

  @override
  State<ArtistScreen> createState() => _ArtistScreenState();
}

class _ArtistScreenState extends State<ArtistScreen> {
  late Future<({Artist artist, List<Album> albums})> _data;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _data = api.getArtist(widget.artistId);
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    final cols = responsiveColumns(context, tile: 170);
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        ScreenHeader(title: widget.title),
        FutureBuilder<({Artist artist, List<Album> albums})>(
          future: _data,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final albums = snap.data?.albums ?? const <Album>[];
            return GridView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: cols,
                crossAxisSpacing: 16,
                mainAxisSpacing: 18,
                childAspectRatio: 0.72,
              ),
              itemCount: albums.length,
              itemBuilder: (context, i) => AlbumCard(
                api: api,
                album: albums[i],
                onTap: () => pushScreen(
                  context,
                  AlbumScreen(
                    controller: widget.controller,
                    albumId: albums[i].id,
                    title: albums[i].name,
                  ),
                ),
              ),
            );
          },
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Album
// ---------------------------------------------------------------------------

class AlbumScreen extends StatefulWidget {
  const AlbumScreen({
    required this.controller,
    required this.albumId,
    required this.title,
    super.key,
  });
  final WaveController controller;
  final String albumId;
  final String title;

  @override
  State<AlbumScreen> createState() => _AlbumScreenState();
}

class _AlbumScreenState extends State<AlbumScreen> {
  late Future<({Album album, List<Song> songs})> _data;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _data = api.getAlbum(widget.albumId);
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        ScreenHeader(title: widget.title),
        FutureBuilder<({Album album, List<Song> songs})>(
          future: _data,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final album = snap.data?.album;
            final songs = snap.data?.songs ?? const <Song>[];
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    CoverArt(
                      api: api,
                      coverArt: album?.coverArt,
                      size: 120,
                      borderRadius: 14,
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            album?.artist ?? '',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              color: kPink,
                              fontWeight: FontWeight.w800,
                            ),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            '${songs.length} треков',
                            style: const TextStyle(
                              color: Colors.white54,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 12),
                          PlayAllButton(
                            onPressed: songs.isEmpty
                                ? null
                                : () => widget.controller.playQueue(songs, 0),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 22),
                for (final song in songs)
                  SongTile(
                    controller: widget.controller,
                    song: song,
                    onTap: () =>
                        widget.controller.playQueue(songs, songs.indexOf(song)),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

class PlaylistsScreen extends StatefulWidget {
  const PlaylistsScreen({required this.controller, super.key});
  final WaveController controller;

  @override
  State<PlaylistsScreen> createState() => _PlaylistsScreenState();
}

class _PlaylistsScreenState extends State<PlaylistsScreen> {
  late Future<List<Playlist>> _playlists;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _playlists = api.getPlaylists();
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    final canPop = Navigator.of(context).canPop();
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        ScreenHeader(title: 'Плейлисты', showBack: canPop),
        FutureBuilder<List<Playlist>>(
          future: _playlists,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final playlists = snap.data ?? const <Playlist>[];
            return Column(
              children: [
                for (final playlist in playlists)
                  PlaylistTile(
                    api: api,
                    playlist: playlist,
                    onTap: () => pushScreen(
                      context,
                      PlaylistScreen(
                        controller: widget.controller,
                        playlistId: playlist.id,
                        title: playlist.name,
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

class PlaylistScreen extends StatefulWidget {
  const PlaylistScreen({
    required this.controller,
    required this.playlistId,
    required this.title,
    super.key,
  });
  final WaveController controller;
  final String playlistId;
  final String title;

  @override
  State<PlaylistScreen> createState() => _PlaylistScreenState();
}

class _PlaylistScreenState extends State<PlaylistScreen> {
  late Future<({Playlist playlist, List<Song> songs})> _data;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _data = api.getPlaylist(widget.playlistId);
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        ScreenHeader(title: widget.title),
        FutureBuilder<({Playlist playlist, List<Song> songs})>(
          future: _data,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final songs = snap.data?.songs ?? const <Song>[];
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                PlayAllButton(
                  onPressed: songs.isEmpty
                      ? null
                      : () => widget.controller.playQueue(songs, 0),
                ),
                const SizedBox(height: 16),
                for (final song in songs)
                  SongTile(
                    controller: widget.controller,
                    song: song,
                    onTap: () =>
                        widget.controller.playQueue(songs, songs.indexOf(song)),
                  ),
              ],
            );
          },
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Liked
// ---------------------------------------------------------------------------

class LikedScreen extends StatefulWidget {
  const LikedScreen({required this.controller, super.key});
  final WaveController controller;

  @override
  State<LikedScreen> createState() => _LikedScreenState();
}

class _LikedScreenState extends State<LikedScreen> {
  late Future<StarredAll> _data;
  SongarrApi get api => widget.controller.api;

  @override
  void initState() {
    super.initState();
    _data = api.getStarred();
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    final cols = responsiveColumns(context, tile: 170);
    return ListView(
      padding: EdgeInsets.fromLTRB(wide ? 36 : 20, 20, wide ? 36 : 20, 140),
      children: [
        const ScreenHeader(title: 'Любимое'),
        FutureBuilder<StarredAll>(
          future: _data,
          builder: (context, snap) {
            if (snap.connectionState != ConnectionState.done) {
              return const StatusView(loading: true, error: null);
            }
            final data = snap.data;
            final songs = data?.songs ?? const <Song>[];
            final albums = data?.albums ?? const <Album>[];
            final artists = data?.artists ?? const <Artist>[];
            if (songs.isEmpty && albums.isEmpty && artists.isEmpty) {
              return const Padding(
                padding: EdgeInsets.symmetric(vertical: 48),
                child: Center(
                  child: Text(
                    'Лайкни трек, альбом или артиста — он появится здесь.',
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      color: Colors.white54,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              );
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (albums.isNotEmpty) ...[
                  const SectionTitle('Альбомы'),
                  GridView.builder(
                    shrinkWrap: true,
                    physics: const NeverScrollableScrollPhysics(),
                    gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                      crossAxisCount: cols,
                      crossAxisSpacing: 16,
                      mainAxisSpacing: 18,
                      childAspectRatio: 0.72,
                    ),
                    itemCount: albums.length,
                    itemBuilder: (context, i) => AlbumCard(
                      api: api,
                      album: albums[i],
                      onTap: () => pushScreen(
                        context,
                        AlbumScreen(
                          controller: widget.controller,
                          albumId: albums[i].id,
                          title: albums[i].name,
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(height: 24),
                ],
                if (artists.isNotEmpty) ...[
                  const SectionTitle('Артисты'),
                  for (final artist in artists)
                    ArtistRow(
                      api: api,
                      artist: artist,
                      onTap: () => pushScreen(
                        context,
                        ArtistScreen(
                          controller: widget.controller,
                          artistId: artist.id,
                          title: artist.name,
                        ),
                      ),
                    ),
                  const SizedBox(height: 24),
                ],
                if (songs.isNotEmpty) ...[
                  const SectionTitle('Песни'),
                  for (final song in songs)
                    SongTile(
                      controller: widget.controller,
                      song: song,
                      onTap: () => widget.controller.playQueue(
                        songs,
                        songs.indexOf(song),
                      ),
                    ),
                ],
              ],
            );
          },
        ),
      ],
    );
  }
}

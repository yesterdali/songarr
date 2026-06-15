import 'package:flutter/material.dart';
import 'package:just_audio/just_audio.dart';

import 'api.dart';
import 'controller.dart';
import 'screens.dart';
import 'theme.dart';
import 'widgets.dart';

class NowPlayingScreen extends StatelessWidget {
  const NowPlayingScreen({required this.controller, super.key});

  final WaveController controller;

  @override
  Widget build(BuildContext context) {
    final api = controller.api;
    return Scaffold(
      backgroundColor: kBlack,
      body: ListenableBuilder(
        listenable: controller,
        builder: (context, _) {
          final song = controller.current;
          if (song == null) return const SizedBox.shrink();
          final player = controller.player;
          final starred = controller.isStarred(song.id);
          return Stack(
            fit: StackFit.expand,
            children: [
              CoverBackdrop(api: api, coverArt: song.coverArt),
              SafeArea(
                child: ListView(
                  padding: const EdgeInsets.fromLTRB(24, 12, 24, 32),
                  children: [
                    Row(
                      children: [
                        CircleAvatar(
                          backgroundColor: Colors.white.withValues(alpha: 0.10),
                          child: IconButton(
                            onPressed: () => Navigator.of(context).pop(),
                            icon: const Icon(Icons.keyboard_arrow_down_rounded),
                          ),
                        ),
                        const Spacer(),
                        const Text(
                          'СЕЙЧАС ИГРАЕТ',
                          style: TextStyle(
                            letterSpacing: 5,
                            color: Colors.white54,
                            fontWeight: FontWeight.w900,
                            fontSize: 12,
                          ),
                        ),
                        const Spacer(),
                        CircleAvatar(
                          backgroundColor: Colors.white.withValues(alpha: 0.10),
                          child: IconButton(
                            tooltip: 'Очередь',
                            onPressed: () => Navigator.of(context).push(
                              MaterialPageRoute<void>(
                                builder: (_) =>
                                    QueueScreen(controller: controller),
                              ),
                            ),
                            icon: const Icon(Icons.queue_music_rounded),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 30),
                    Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 380),
                        child: PulseScale(
                          active: controller.isPlaying,
                          maxScale: 1.03,
                          child: AspectRatio(
                            aspectRatio: 1,
                            child: LayoutBuilder(
                              builder: (context, c) => CoverArt(
                                api: api,
                                coverArt: song.coverArt,
                                size: c.maxWidth,
                                borderRadius: 22,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(height: 30),
                    Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 560),
                        child: Column(
                          children: [
                            _TitleRow(
                              controller: controller,
                              song: song,
                              starred: starred,
                            ),
                            const SizedBox(height: 22),
                            _Scrubber(player: player, song: song),
                            const SizedBox(height: 18),
                            _Transport(controller: controller, player: player),
                            const SizedBox(height: 20),
                            OutlinedButton.icon(
                              onPressed: () => Navigator.of(context).push(
                                MaterialPageRoute<void>(
                                  builder: (_) => LyricsScreen(
                                    controller: controller,
                                    song: song,
                                  ),
                                ),
                              ),
                              style: OutlinedButton.styleFrom(
                                foregroundColor: kCream,
                                side: BorderSide(
                                  color: Colors.white.withValues(alpha: 0.16),
                                ),
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 18,
                                  vertical: 12,
                                ),
                                shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(999),
                                ),
                              ),
                              icon: const Icon(Icons.lyrics_rounded, size: 20),
                              label: const Text(
                                'Текст',
                                style: TextStyle(fontWeight: FontWeight.w800),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _TitleRow extends StatelessWidget {
  const _TitleRow({
    required this.controller,
    required this.song,
    required this.starred,
  });
  final WaveController controller;
  final Song song;
  final bool starred;

  void _openArtist(BuildContext context) {
    if (song.artistId == null || song.artistId!.isEmpty) {
      pushScreen(
        context,
        SearchScreen(
          controller: controller,
          initialQuery: song.artist,
          showBack: true,
        ),
      );
      return;
    }
    pushScreen(
      context,
      ArtistScreen(
        controller: controller,
        artistId: song.artistId!,
        title: song.artist,
      ),
    );
  }

  void _openAlbum(BuildContext context) {
    if (song.album == null || song.album!.isEmpty) return;
    if (song.albumId == null || song.albumId!.isEmpty) {
      pushScreen(
        context,
        SearchScreen(
          controller: controller,
          initialQuery: '${song.artist} ${song.album}',
          showBack: true,
        ),
      );
      return;
    }
    pushScreen(
      context,
      AlbumScreen(
        controller: controller,
        albumId: song.albumId!,
        title: song.album!,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                song.title,
                style: serif(fontSize: 32, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 6),
              Row(
                children: [
                  if (song.provider == 'yandex') const ProviderPill(),
                  Flexible(
                    child: GestureDetector(
                      onTap: () => _openArtist(context),
                      child: Text(
                        song.artist,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 18,
                          color: Colors.white70,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
              if (song.album != null) ...[
                const SizedBox(height: 4),
                GestureDetector(
                  onTap: () => _openAlbum(context),
                  child: Text(
                    song.album!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontSize: 15,
                      color: Colors.white38,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ],
            ],
          ),
        ),
        IconButton(
          onPressed: () => controller.toggleStar(song.id),
          iconSize: 28,
          icon: Icon(
            starred ? Icons.favorite_rounded : Icons.favorite_border_rounded,
            color: starred ? kPink : Colors.white60,
          ),
        ),
        IconButton(
          onPressed: controller.dislikeCurrent,
          iconSize: 28,
          icon: const Icon(Icons.not_interested_rounded, color: Colors.white60),
        ),
      ],
    );
  }
}

class _Scrubber extends StatelessWidget {
  const _Scrubber({required this.player, required this.song});
  final AudioPlayer player;
  final Song song;

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<Duration>(
      stream: player.positionStream,
      initialData: player.position,
      builder: (context, snapshot) {
        final position = snapshot.data ?? Duration.zero;
        final duration =
            player.duration ?? Duration(seconds: song.duration ?? 1);
        final maxMs = duration.inMilliseconds.toDouble().clamp(
          1.0,
          double.infinity,
        );
        return Column(
          children: [
            SliderTheme(
              data: SliderTheme.of(context).copyWith(
                trackHeight: 4,
                thumbShape: const DiamondThumb(),
                overlayShape: SliderComponentShape.noOverlay,
                activeTrackColor: kPink,
                inactiveTrackColor: kCream.withValues(alpha: 0.16),
              ),
              child: Slider(
                min: 0,
                max: maxMs,
                value: position.inMilliseconds.toDouble().clamp(0.0, maxMs),
                onChanged: (value) =>
                    player.seek(Duration(milliseconds: value.round())),
              ),
            ),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text(
                  formatDuration(position),
                  style: const TextStyle(
                    color: Colors.white54,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                Text(
                  formatDuration(duration),
                  style: const TextStyle(
                    color: Colors.white54,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ],
            ),
          ],
        );
      },
    );
  }
}

class _Transport extends StatelessWidget {
  const _Transport({required this.controller, required this.player});
  final WaveController controller;
  final AudioPlayer player;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        IconButton(
          onPressed: controller.previous,
          iconSize: 44,
          icon: const Icon(Icons.skip_previous_rounded),
        ),
        const SizedBox(width: 26),
        IconButton(
          onPressed: controller.toggle,
          iconSize: 50,
          style: IconButton.styleFrom(
            backgroundColor: kCream,
            foregroundColor: kBlack,
            fixedSize: const Size(82, 82),
          ),
          icon: PlayPauseIcon(playing: player.playing, size: 34),
        ),
        const SizedBox(width: 26),
        IconButton(
          onPressed: () => controller.next(),
          iconSize: 44,
          icon: const Icon(Icons.skip_next_rounded),
        ),
      ],
    );
  }
}

class QueueScreen extends StatelessWidget {
  const QueueScreen({required this.controller, super.key});

  final WaveController controller;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: kBlack,
      body: ListenableBuilder(
        listenable: controller,
        builder: (context, _) {
          final current = controller.current;
          final queue = controller.queue;
          return Stack(
            fit: StackFit.expand,
            children: [
              CoverBackdrop(api: controller.api, coverArt: current?.coverArt),
              SafeArea(
                child: ListView(
                  padding: const EdgeInsets.fromLTRB(20, 12, 20, 36),
                  children: [
                    Row(
                      children: [
                        CircleAvatar(
                          backgroundColor: Colors.white.withValues(alpha: 0.10),
                          child: IconButton(
                            onPressed: () => Navigator.of(context).pop(),
                            icon: const Icon(Icons.arrow_back_rounded),
                          ),
                        ),
                        const SizedBox(width: 14),
                        Expanded(
                          child: Text(
                            'Далее',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: serif(
                              fontSize: 32,
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                        ),
                        Text(
                          '${queue.length}',
                          style: const TextStyle(
                            color: Colors.white54,
                            fontWeight: FontWeight.w900,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 24),
                    if (queue.isEmpty)
                      const Padding(
                        padding: EdgeInsets.only(top: 80),
                        child: Center(
                          child: Text(
                            'Очередь пуста.',
                            style: TextStyle(
                              color: Colors.white54,
                              fontWeight: FontWeight.w800,
                            ),
                          ),
                        ),
                      )
                    else ...[
                      const _QueueSectionLabel('СЕЙЧАС'),
                      SongTile(
                        controller: controller,
                        song: queue[controller.index],
                        onTap: () => controller.playAt(controller.index),
                      ),
                      const SizedBox(height: 18),
                      const _QueueSectionLabel('ДАЛЕЕ'),
                      if (controller.index + 1 >= queue.length)
                        const Padding(
                          padding: EdgeInsets.symmetric(vertical: 18),
                          child: Text(
                            'Новые рекомендации появятся по мере прослушивания.',
                            style: TextStyle(
                              color: Colors.white38,
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                        )
                      else
                        for (
                          var i = controller.index + 1;
                          i < queue.length;
                          i++
                        )
                          SongTile(
                            controller: controller,
                            song: queue[i],
                            onTap: () => controller.playAt(i),
                          ),
                    ],
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _QueueSectionLabel extends StatelessWidget {
  const _QueueSectionLabel(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Text(
        text,
        style: const TextStyle(
          color: Colors.white38,
          fontSize: 12,
          fontWeight: FontWeight.w900,
          letterSpacing: 3,
        ),
      ),
    );
  }
}

class LyricsScreen extends StatefulWidget {
  const LyricsScreen({required this.controller, required this.song, super.key});

  final WaveController controller;
  final Song song;

  @override
  State<LyricsScreen> createState() => _LyricsScreenState();
}

class _LyricsScreenState extends State<LyricsScreen> {
  late Future<LyricsResult?> _future;

  @override
  void initState() {
    super.initState();
    _future = widget.controller.api.getLyrics(widget.song.id);
  }

  int _activeIndex(LyricsResult lyrics, int positionMs) {
    if (!lyrics.synced) return -1;
    final nowMs = positionMs + 150;
    var active = -1;
    for (var i = 0; i < lyrics.lines.length; i++) {
      final start = lyrics.lines[i].start;
      if (start == null || start > nowMs) break;
      active = i;
    }
    return active;
  }

  @override
  Widget build(BuildContext context) {
    final song = widget.song;
    final api = widget.controller.api;
    return Scaffold(
      backgroundColor: kBlack,
      body: Stack(
        fit: StackFit.expand,
        children: [
          CoverBackdrop(api: api, coverArt: song.coverArt),
          SafeArea(
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 10, 16, 4),
                  child: Row(
                    children: [
                      CircleAvatar(
                        backgroundColor: Colors.white.withValues(alpha: 0.10),
                        child: IconButton(
                          onPressed: () => Navigator.of(context).pop(),
                          icon: const Icon(Icons.arrow_back_rounded),
                        ),
                      ),
                      const SizedBox(width: 12),
                      CoverArt(
                        api: api,
                        coverArt: song.coverArt,
                        size: 48,
                        borderRadius: 10,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              song.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontWeight: FontWeight.w900,
                                color: kCream,
                              ),
                            ),
                            Text(
                              song.artist,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                color: Colors.white54,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
                Expanded(
                  child: FutureBuilder<LyricsResult?>(
                    future: _future,
                    builder: (context, snapshot) {
                      if (snapshot.connectionState != ConnectionState.done) {
                        return const Center(
                          child: Text(
                            'Ищем текст...',
                            style: TextStyle(
                              color: Colors.white54,
                              fontWeight: FontWeight.w800,
                            ),
                          ),
                        );
                      }
                      final lyrics = snapshot.data;
                      if (lyrics == null || lyrics.lines.isEmpty) {
                        return const Center(
                          child: Padding(
                            padding: EdgeInsets.all(32),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Icon(
                                  Icons.lyrics_outlined,
                                  size: 40,
                                  color: Colors.white24,
                                ),
                                SizedBox(height: 14),
                                Text(
                                  'Текста пока нет.',
                                  style: TextStyle(
                                    color: Colors.white60,
                                    fontWeight: FontWeight.w900,
                                    fontSize: 18,
                                  ),
                                ),
                              ],
                            ),
                          ),
                        );
                      }
                      return StreamBuilder<Duration>(
                        stream: widget.controller.player.positionStream,
                        initialData: widget.controller.player.position,
                        builder: (context, snap) {
                          final positionMs =
                              (snap.data ?? Duration.zero).inMilliseconds;
                          final active = _activeIndex(lyrics, positionMs);
                          return ListView.builder(
                            padding: const EdgeInsets.fromLTRB(24, 8, 24, 60),
                            itemCount: lyrics.lines.length,
                            itemBuilder: (context, i) {
                              final line = lyrics.lines[i];
                              final isActive = i == active;
                              final canSeek =
                                  lyrics.synced && line.start != null;
                              return GestureDetector(
                                behavior: HitTestBehavior.opaque,
                                onTap: canSeek
                                    ? () => widget.controller.player.seek(
                                        Duration(milliseconds: line.start!),
                                      )
                                    : null,
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(
                                    vertical: 8,
                                  ),
                                  child: Text(
                                    line.value,
                                    style: TextStyle(
                                      fontSize: 24,
                                      height: 1.2,
                                      fontWeight: FontWeight.w900,
                                      color: isActive
                                          ? kCream
                                          : Colors.white.withValues(
                                              alpha: 0.36,
                                            ),
                                    ),
                                  ),
                                ),
                              );
                            },
                          );
                        },
                      );
                    },
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

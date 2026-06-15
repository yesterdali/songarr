import 'dart:math' as math;

import 'package:flutter/material.dart';

import 'api.dart';
import 'controller.dart';
import 'theme.dart';

/// Animated "now playing" equalizer — bars bounce while [playing], and freeze
/// short when paused. The clearest signal that a track is live.
class EqualizerBars extends StatefulWidget {
  const EqualizerBars({
    this.playing = true,
    this.color = kCream,
    this.height = 18,
    this.barCount = 4,
    super.key,
  });

  final bool playing;
  final Color color;
  final double height;
  final int barCount;

  @override
  State<EqualizerBars> createState() => _EqualizerBarsState();
}

class _EqualizerBarsState extends State<EqualizerBars>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1000),
  );

  @override
  void initState() {
    super.initState();
    if (widget.playing) _c.repeat();
  }

  @override
  void didUpdateWidget(EqualizerBars old) {
    super.didUpdateWidget(old);
    if (widget.playing && !_c.isAnimating) {
      _c.repeat();
    } else if (!widget.playing && _c.isAnimating) {
      _c.stop();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final barWidth = widget.height / 5.5;
    return AnimatedBuilder(
      animation: _c,
      builder: (context, _) {
        return Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            for (var i = 0; i < widget.barCount; i++) ...[
              if (i > 0) SizedBox(width: barWidth * 0.7),
              _bar(i, barWidth),
            ],
          ],
        );
      },
    );
  }

  Widget _bar(int i, double barWidth) {
    final factor = widget.playing
        ? 0.25 + 0.75 * (0.5 + 0.5 * math.sin(2 * math.pi * (_c.value + i * 0.25)))
        : 0.35;
    return Container(
      width: barWidth,
      height: (widget.height * factor).clamp(2.0, widget.height),
      decoration: BoxDecoration(
        color: widget.color,
        borderRadius: BorderRadius.circular(barWidth),
      ),
    );
  }
}

/// Gently breathes its child (slow scale pulse) while [active]; still otherwise.
class PulseScale extends StatefulWidget {
  const PulseScale({
    required this.child,
    this.active = true,
    this.maxScale = 1.04,
    super.key,
  });

  final Widget child;
  final bool active;
  final double maxScale;

  @override
  State<PulseScale> createState() => _PulseScaleState();
}

class _PulseScaleState extends State<PulseScale>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1700),
  );

  @override
  void initState() {
    super.initState();
    if (widget.active) _c.repeat(reverse: true);
  }

  @override
  void didUpdateWidget(PulseScale old) {
    super.didUpdateWidget(old);
    if (widget.active && !_c.isAnimating) {
      _c.repeat(reverse: true);
    } else if (!widget.active && _c.isAnimating) {
      _c.animateTo(0, duration: const Duration(milliseconds: 300));
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ScaleTransition(
      scale: Tween(begin: 1.0, end: widget.maxScale)
          .animate(CurvedAnimation(parent: _c, curve: Curves.easeInOut)),
      child: widget.child,
    );
  }
}

/// Thin live progress bar driven by the player's position — the "playbar".
class PlaybackProgressBar extends StatelessWidget {
  const PlaybackProgressBar({required this.controller, this.height = 3, super.key});

  final WaveController controller;
  final double height;

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<Duration>(
      stream: controller.player.positionStream,
      initialData: controller.player.position,
      builder: (context, snap) {
        final pos = snap.data ?? Duration.zero;
        final dur = controller.player.duration ??
            Duration(seconds: controller.current?.duration ?? 0);
        final frac = dur.inMilliseconds <= 0
            ? 0.0
            : (pos.inMilliseconds / dur.inMilliseconds).clamp(0.0, 1.0);
        return ClipRRect(
          borderRadius: BorderRadius.circular(height),
          child: Stack(
            children: [
              Container(height: height, color: kCream.withValues(alpha: 0.12)),
              FractionallySizedBox(
                widthFactor: frac,
                child: Container(
                  height: height,
                  decoration: const BoxDecoration(
                    gradient: LinearGradient(colors: [kDeepPink, kPink]),
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Play/pause icon that morphs between states with a quick scale-fade.
class PlayPauseIcon extends StatelessWidget {
  const PlayPauseIcon({required this.playing, this.size = 24, super.key});
  final bool playing;
  final double size;

  @override
  Widget build(BuildContext context) {
    return AnimatedSwitcher(
      duration: const Duration(milliseconds: 220),
      transitionBuilder: (child, anim) =>
          ScaleTransition(scale: anim, child: FadeTransition(opacity: anim, child: child)),
      child: Icon(
        playing ? Icons.pause_rounded : Icons.play_arrow_rounded,
        key: ValueKey(playing),
        size: size,
      ),
    );
  }
}

class CoverArt extends StatelessWidget {
  const CoverArt({
    required this.api,
    required this.coverArt,
    required this.size,
    this.borderRadius = 12,
    super.key,
  });

  final SongarrApi api;
  final String? coverArt;
  final double size;
  final double borderRadius;

  @override
  Widget build(BuildContext context) {
    final url = api.coverUrl(coverArt, size: size.round().clamp(64, 1024));
    return ClipRRect(
      borderRadius: BorderRadius.circular(borderRadius),
      child: SizedBox.square(
        dimension: size,
        child: url.isEmpty
            ? const _CoverPlaceholder()
            : Image.network(
                url,
                // Key by URL: on web, a reused Image element with a changed src
                // can keep showing the old cover — a fresh element per URL loads
                // the correct one.
                key: ValueKey(url),
                fit: BoxFit.cover,
                errorBuilder: (_, _, _) => const _CoverPlaceholder(),
                frameBuilder: (context, child, frame, wasSync) {
                  if (wasSync || frame != null) return child;
                  return const _CoverPlaceholder();
                },
              ),
      ),
    );
  }
}

class _CoverPlaceholder extends StatelessWidget {
  const _CoverPlaceholder();

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        gradient: LinearGradient(colors: [kDeepPink, Color(0xff3d1024)]),
      ),
      child: Icon(
        Icons.music_note_rounded,
        color: kCream.withValues(alpha: 0.7),
      ),
    );
  }
}

class CoverBackdrop extends StatelessWidget {
  const CoverBackdrop({required this.api, required this.coverArt, super.key});

  final SongarrApi api;
  final String? coverArt;

  @override
  Widget build(BuildContext context) {
    final url = api.coverUrl(coverArt, size: 120);
    return Stack(
      fit: StackFit.expand,
      children: [
        if (url.isNotEmpty)
          Image.network(
            url,
            key: ValueKey(url),
            fit: BoxFit.cover,
            errorBuilder: (_, _, _) => const SizedBox.shrink(),
          ),
        const DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [Color(0xdd1b0a0d), Color(0xee12070b), kBlack],
            ),
          ),
        ),
      ],
    );
  }
}

class InkSurface extends StatelessWidget {
  const InkSurface({required this.child, super.key});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Material(type: MaterialType.transparency, child: child);
  }
}

class PageSurface extends StatelessWidget {
  const PageSurface({required this.child, super.key});
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final baseStyle =
        Theme.of(context).textTheme.bodyMedium ?? const TextStyle();
    return Material(
      color: kBlack,
      child: DefaultTextStyle(
        style: baseStyle.copyWith(
          color: kCream,
          decoration: TextDecoration.none,
        ),
        child: IconTheme.merge(
          data: const IconThemeData(color: kCream),
          child: child,
        ),
      ),
    );
  }
}

class ProviderPill extends StatelessWidget {
  const ProviderPill({this.small = false, super.key});
  final bool small;

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: EdgeInsets.only(right: small ? 6 : 10),
      padding: EdgeInsets.symmetric(
        horizontal: small ? 7 : 10,
        vertical: small ? 2 : 4,
      ),
      decoration: BoxDecoration(
        color: kPink.withValues(alpha: 0.18),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        'YANDEX',
        style: TextStyle(
          color: kPink,
          fontSize: small ? 9 : 11,
          fontWeight: FontWeight.w900,
          letterSpacing: 1.6,
        ),
      ),
    );
  }
}

/// A song row that reflects play state (highlight + equalizer) and exposes a
/// heart. Listens to the controller itself so any list stays live.
class SongTile extends StatelessWidget {
  const SongTile({
    required this.controller,
    required this.song,
    required this.onTap,
    super.key,
  });

  final WaveController controller;
  final Song song;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final api = controller.api;
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final active = controller.current?.id == song.id;
        final starred = controller.isStarred(song.id);
        return InkSurface(
          child: InkWell(
            borderRadius: BorderRadius.circular(14),
            onTap: onTap,
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 6),
              child: Row(
                children: [
                  Stack(
                    alignment: Alignment.center,
                    children: [
                      CoverArt(
                        api: api,
                        coverArt: song.coverArt,
                        size: 52,
                        borderRadius: 11,
                      ),
                      if (active)
                        Container(
                          width: 52,
                          height: 52,
                          alignment: Alignment.center,
                          decoration: BoxDecoration(
                            color: Colors.black.withValues(alpha: 0.45),
                            borderRadius: BorderRadius.circular(11),
                          ),
                          child: controller.isPlaying
                              ? const EqualizerBars(height: 18)
                              : const Icon(Icons.pause_rounded, color: kCream, size: 22),
                        ),
                    ],
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
                          style: TextStyle(
                            fontWeight: FontWeight.w800,
                            color: active ? kPink : kCream,
                          ),
                        ),
                        const SizedBox(height: 4),
                        Row(
                          children: [
                            if (song.provider == 'yandex')
                              const ProviderPill(small: true),
                            Flexible(
                              child: Text(
                                song.artist,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  color: Colors.white54,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                  IconButton(
                    visualDensity: VisualDensity.compact,
                    onPressed: () => controller.toggleStar(song.id),
                    icon: Icon(
                      starred
                          ? Icons.favorite_rounded
                          : Icons.favorite_border_rounded,
                      color: starred ? kPink : Colors.white38,
                      size: 20,
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class AlbumCard extends StatelessWidget {
  const AlbumCard({
    required this.api,
    required this.album,
    required this.onTap,
    this.width,
    super.key,
  });

  final SongarrApi api;
  final Album album;
  final VoidCallback onTap;
  final double? width;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: width,
      child: InkSurface(
        child: InkWell(
          borderRadius: BorderRadius.circular(14),
          onTap: onTap,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              LayoutBuilder(
                builder: (context, constraints) {
                  final side = width ?? constraints.maxWidth;
                  return CoverArt(
                    api: api,
                    coverArt: album.coverArt,
                    size: side.isFinite ? side : 150,
                    borderRadius: 12,
                  );
                },
              ),
              const SizedBox(height: 8),
              Text(
                album.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  fontWeight: FontWeight.w800,
                  color: kCream,
                ),
              ),
              Text(
                album.artist,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: Colors.white54,
                  fontWeight: FontWeight.w600,
                  fontSize: 13,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class ArtistRow extends StatelessWidget {
  const ArtistRow({
    required this.api,
    required this.artist,
    required this.onTap,
    super.key,
  });

  final SongarrApi api;
  final Artist artist;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final initial = artist.name.isEmpty
        ? '?'
        : artist.name.substring(0, 1).toUpperCase();
    final url = api.coverUrl(artist.coverArt, size: 96);
    return InkSurface(
      child: InkWell(
        borderRadius: BorderRadius.circular(14),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 6),
          child: Row(
            children: [
              SizedBox(
                width: 48,
                height: 48,
                child: ClipOval(
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      // Gradient initial — the base, and the fallback if the
                      // artist has no image or it fails to load.
                      DecoratedBox(
                        decoration: const BoxDecoration(
                          gradient: LinearGradient(
                            begin: Alignment.topLeft,
                            end: Alignment.bottomRight,
                            colors: [Color(0xcca4243b), Color(0xcc45122e)],
                          ),
                        ),
                        child: Center(
                          child: Text(
                            initial,
                            style: const TextStyle(
                              fontWeight: FontWeight.w900,
                              color: kCream,
                            ),
                          ),
                        ),
                      ),
                      if (url.isNotEmpty)
                        Image.network(
                          url,
                          key: ValueKey(url),
                          fit: BoxFit.cover,
                          errorBuilder: (_, _, _) => const SizedBox.shrink(),
                          frameBuilder: (context, child, frame, wasSync) =>
                              wasSync || frame != null
                              ? child
                              : const SizedBox.shrink(),
                        ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  artist.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontWeight: FontWeight.w800,
                    color: kCream,
                  ),
                ),
              ),
              const Icon(Icons.chevron_right_rounded, color: Colors.white24),
            ],
          ),
        ),
      ),
    );
  }
}

class PlaylistTile extends StatelessWidget {
  const PlaylistTile({
    required this.api,
    required this.playlist,
    required this.onTap,
    super.key,
  });

  final SongarrApi api;
  final Playlist playlist;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return InkSurface(
      child: InkWell(
        borderRadius: BorderRadius.circular(14),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 6),
          child: Row(
            children: [
              CoverArt(
                api: api,
                coverArt: playlist.coverArt,
                size: 52,
                borderRadius: 11,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      playlist.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        fontWeight: FontWeight.w800,
                        color: kCream,
                      ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      '${playlist.songCount ?? 0} треков',
                      style: const TextStyle(
                        color: Colors.white54,
                        fontWeight: FontWeight.w600,
                        fontSize: 13,
                      ),
                    ),
                  ],
                ),
              ),
              const Icon(Icons.chevron_right_rounded, color: Colors.white24),
            ],
          ),
        ),
      ),
    );
  }
}

class PlayAllButton extends StatelessWidget {
  const PlayAllButton({required this.onPressed, super.key});
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(999),
        gradient: const LinearGradient(colors: [Color(0xffa4243b), kPink]),
        boxShadow: [
          BoxShadow(
            color: kPink.withValues(alpha: 0.3),
            blurRadius: 18,
            offset: const Offset(0, 8),
          ),
        ],
      ),
      child: TextButton.icon(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          foregroundColor: Colors.white,
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 12),
        ),
        icon: const Icon(Icons.play_arrow_rounded),
        label: const Text(
          'Слушать',
          style: TextStyle(fontWeight: FontWeight.w900),
        ),
      ),
    );
  }
}

/// A screen header with an optional back button and a serif title.
class ScreenHeader extends StatelessWidget {
  const ScreenHeader({required this.title, this.showBack = true, super.key});
  final String title;
  final bool showBack;

  @override
  Widget build(BuildContext context) {
    final canPop = showBack && Navigator.of(context).canPop();
    return Padding(
      padding: const EdgeInsets.only(bottom: 18),
      child: Row(
        children: [
          if (canPop) ...[
            CircleAvatar(
              backgroundColor: Colors.white.withValues(alpha: 0.06),
              radius: 20,
              child: IconButton(
                onPressed: () => Navigator.of(context).pop(),
                icon: const Icon(Icons.chevron_left_rounded),
                color: kCream,
              ),
            ),
            const SizedBox(width: 12),
          ],
          Expanded(
            child: Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: serif(fontSize: 30, fontWeight: FontWeight.w700),
            ),
          ),
        ],
      ),
    );
  }
}

/// Centered spinner / error used by data screens.
class StatusView extends StatelessWidget {
  const StatusView({required this.loading, required this.error, super.key});
  final bool loading;
  final String? error;

  @override
  Widget build(BuildContext context) {
    if (loading) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 40),
        child: Center(
          child: SizedBox.square(
            dimension: 28,
            child: CircularProgressIndicator(strokeWidth: 3, color: kPink),
          ),
        ),
      );
    }
    if (error != null) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 24),
        child: Text(error!, style: const TextStyle(color: Colors.redAccent)),
      );
    }
    return const SizedBox.shrink();
  }
}

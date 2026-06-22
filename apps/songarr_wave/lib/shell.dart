import 'dart:ui';

import 'package:flutter/material.dart';

import 'api.dart';
import 'controller.dart';
import 'player_screens.dart';
import 'screens.dart';
import 'theme.dart';
import 'widgets.dart';

class _Tab {
  const _Tab(this.icon, this.label);
  final IconData icon;
  final String label;
}

const _tabs = [
  _Tab(Icons.graphic_eq_rounded, 'Волна'),
  _Tab(Icons.search_rounded, 'Поиск'),
  _Tab(Icons.library_music_rounded, 'Медиатека'),
  _Tab(Icons.queue_music_rounded, 'Плейлисты'),
];

class WaveShell extends StatefulWidget {
  const WaveShell({required this.api, required this.onLogout, super.key});

  final SongarrApi api;
  final VoidCallback onLogout;

  @override
  State<WaveShell> createState() => _WaveShellState();
}

class _WaveShellState extends State<WaveShell> {
  late final WaveController _controller;
  final _navKeys = List.generate(
    _tabs.length,
    (_) => GlobalKey<NavigatorState>(),
  );
  int _tab = 0;

  @override
  void initState() {
    super.initState();
    _controller = WaveController(widget.api);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Widget _rootScreen(int i) {
    switch (i) {
      case 0:
        return HomeScreen(
          controller: _controller,
          username: widget.api.session.username,
          onLogout: widget.onLogout,
        );
      case 1:
        return SearchScreen(controller: _controller);
      case 2:
        return LibraryScreen(controller: _controller);
      default:
        return PlaylistsScreen(controller: _controller);
    }
  }

  void _selectTab(int i) {
    if (i == _tab) {
      // Tapping the active tab pops to its root.
      _navKeys[i].currentState?.popUntil((route) => route.isFirst);
    } else {
      setState(() => _tab = i);
    }
  }

  void _openNowPlaying() {
    if (_controller.current == null) return;
    Navigator.of(context, rootNavigator: true).push(
      MaterialPageRoute<void>(
        builder: (_) => NowPlayingScreen(controller: _controller),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final tabs = IndexedStack(
      index: _tab,
      children: [
        for (var i = 0; i < _tabs.length; i++)
          Navigator(
            key: _navKeys[i],
            onGenerateRoute: (settings) => wavePageRoute(_rootScreen(i)),
          ),
      ],
    );

    final wide = MediaQuery.of(context).size.width > kDesktopBreakpoint;
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: wide ? _wideLayout(tabs) : _narrowLayout(tabs),
    );
  }

  // Desktop: left nav rail + content, full-width now-playing bar at the bottom.
  Widget _wideLayout(Widget tabs) {
    return Column(
      children: [
        Expanded(
          child: Row(
            children: [
              _NavRail(tabs: _tabs, current: _tab, onSelect: _selectTab),
              Expanded(child: SafeArea(left: false, child: tabs)),
            ],
          ),
        ),
        ListenableBuilder(
          listenable: _controller,
          builder: (context, _) {
            if (_controller.current == null) return const SizedBox.shrink();
            return _DockSurface(
              radius: 0,
              child: DesktopPlayerBar(
                controller: _controller,
                onOpen: _openNowPlaying,
              ),
            );
          },
        ),
      ],
    );
  }

  // Mobile: content + floating glass dock (mini-player above the tab row).
  Widget _narrowLayout(Widget tabs) {
    return SafeArea(
      child: Stack(
        children: [
          Positioned.fill(child: tabs),
          Positioned(
            left: 12,
            right: 12,
            bottom: 12,
            child: _DockSurface(
              radius: 22,
              child: ListenableBuilder(
                listenable: _controller,
                builder: (context, _) {
                  return Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (_controller.current != null)
                        MiniPlayer(
                          controller: _controller,
                          onOpen: _openNowPlaying,
                        ),
                      _TabBar(tabs: _tabs, current: _tab, onSelect: _selectTab),
                    ],
                  );
                },
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Frosted translucent surface used by both the mobile dock and desktop bar.
class _DockSurface extends StatelessWidget {
  const _DockSurface({required this.child, required this.radius});
  final Widget child;
  final double radius;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 24, sigmaY: 24),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: const Color(0xff0d070b).withValues(alpha: 0.88),
            borderRadius: BorderRadius.circular(radius),
            border: Border.all(color: Colors.white.withValues(alpha: 0.08)),
          ),
          child: child,
        ),
      ),
    );
  }
}

class _TabBar extends StatelessWidget {
  const _TabBar({
    required this.tabs,
    required this.current,
    required this.onSelect,
  });
  final List<_Tab> tabs;
  final int current;
  final ValueChanged<int> onSelect;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          for (var i = 0; i < tabs.length; i++)
            Expanded(
              child: InkSurface(
                child: InkWell(
                  borderRadius: BorderRadius.circular(14),
                  onTap: () => onSelect(i),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          tabs[i].icon,
                          color: i == current ? kPink : Colors.white54,
                          size: 24,
                        ),
                        const SizedBox(height: 3),
                        Text(
                          tabs[i].label,
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w800,
                            color: i == current ? kPink : Colors.white54,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _NavRail extends StatelessWidget {
  const _NavRail({
    required this.tabs,
    required this.current,
    required this.onSelect,
  });
  final List<_Tab> tabs;
  final int current;
  final ValueChanged<int> onSelect;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 232,
      decoration: BoxDecoration(
        border: Border(
          right: BorderSide(color: Colors.white.withValues(alpha: 0.06)),
        ),
      ),
      child: SafeArea(
        right: false,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 24, 24, 28),
              child: Row(
                children: [
                  Container(
                    width: 38,
                    height: 38,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(11),
                      gradient: const LinearGradient(
                        colors: [Color(0xffa4243b), kViolet],
                      ),
                    ),
                    child: const Icon(
                      Icons.graphic_eq_rounded,
                      color: kCream,
                      size: 20,
                    ),
                  ),
                  const SizedBox(width: 12),
                  Text(
                    'Волна',
                    style: serif(fontSize: 24, fontWeight: FontWeight.w700),
                  ),
                ],
              ),
            ),
            for (var i = 0; i < tabs.length; i++)
              _RailItem(
                tab: tabs[i],
                selected: i == current,
                onTap: () => onSelect(i),
              ),
          ],
        ),
      ),
    );
  }
}

class _RailItem extends StatelessWidget {
  const _RailItem({
    required this.tab,
    required this.selected,
    required this.onTap,
  });
  final _Tab tab;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 3),
      child: InkSurface(
        child: InkWell(
          borderRadius: BorderRadius.circular(12),
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 13),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(12),
              color: selected
                  ? kPink.withValues(alpha: 0.16)
                  : Colors.transparent,
            ),
            child: Row(
              children: [
                Icon(
                  tab.icon,
                  color: selected ? kPink : Colors.white60,
                  size: 22,
                ),
                const SizedBox(width: 14),
                Text(
                  tab.label,
                  style: TextStyle(
                    fontWeight: FontWeight.w800,
                    color: selected ? kCream : Colors.white60,
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

class MiniPlayer extends StatelessWidget {
  const MiniPlayer({required this.controller, required this.onOpen, super.key});
  final WaveController controller;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final song = controller.current;
    if (song == null) return const SizedBox.shrink();
    final api = controller.api;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(14, 10, 10, 8),
          child: Row(
            children: [
              Stack(
                alignment: Alignment.bottomLeft,
                children: [
                  PulseScale(
                    active: controller.isPlaying,
                    child: CoverArt(
                      api: api,
                      coverArt: song.coverArt,
                      size: 50,
                      borderRadius: 11,
                    ),
                  ),
                  if (controller.isPlaying)
                    Positioned(
                      left: 4,
                      bottom: 4,
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 3),
                        decoration: BoxDecoration(
                          color: Colors.black.withValues(alpha: 0.55),
                          borderRadius: BorderRadius.circular(6),
                        ),
                        child: const EqualizerBars(height: 11, color: kPink, barCount: 3),
                      ),
                    ),
                ],
              ),
              const SizedBox(width: 12),
          Expanded(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: onOpen,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    song.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontWeight: FontWeight.w800,
                      color: kCream,
                    ),
                  ),
                  const SizedBox(height: 2),
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
          ),
              IconButton(
                onPressed: controller.toggle,
                icon: PlayPauseIcon(playing: controller.isPlaying),
                style: IconButton.styleFrom(
                  backgroundColor: kCream,
                  foregroundColor: kBlack,
                ),
              ),
              IconButton(
                onPressed: () => controller.next(),
                icon: const Icon(Icons.skip_next_rounded),
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
          child: PlaybackProgressBar(controller: controller),
        ),
      ],
    );
  }
}

/// Spotify-style desktop player bar: track info (left), transport + seek
/// (center), visualizer + volume (right).
class DesktopPlayerBar extends StatelessWidget {
  const DesktopPlayerBar({required this.controller, required this.onOpen, super.key});
  final WaveController controller;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final song = controller.current;
    if (song == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Row(
        children: [
          Expanded(flex: 3, child: _BarTrackInfo(controller: controller, onOpen: onOpen)),
          Expanded(flex: 4, child: _BarCenter(controller: controller)),
          Expanded(flex: 3, child: _BarRight(controller: controller)),
        ],
      ),
    );
  }
}

class _BarTrackInfo extends StatelessWidget {
  const _BarTrackInfo({required this.controller, required this.onOpen});
  final WaveController controller;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final song = controller.current!;
    final api = controller.api;
    final starred = controller.isStarred(song.id);
    return Row(
      children: [
        Stack(
          alignment: Alignment.bottomLeft,
          children: [
            PulseScale(
              active: controller.isPlaying,
              child: CoverArt(api: api, coverArt: song.coverArt, size: 54, borderRadius: 10),
            ),
            if (controller.isPlaying)
              Positioned(
                left: 4,
                bottom: 4,
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 3),
                  decoration: BoxDecoration(
                    color: Colors.black.withValues(alpha: 0.55),
                    borderRadius: BorderRadius.circular(6),
                  ),
                  child: const EqualizerBars(height: 11, color: kPink, barCount: 3),
                ),
              ),
          ],
        ),
        const SizedBox(width: 12),
        Expanded(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: onOpen,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(song.title, maxLines: 1, overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontWeight: FontWeight.w800, color: kCream)),
                const SizedBox(height: 2),
                Text(song.artist, maxLines: 1, overflow: TextOverflow.ellipsis,
                    style: const TextStyle(color: Colors.white54, fontWeight: FontWeight.w600, fontSize: 13)),
              ],
            ),
          ),
        ),
        IconButton(
          visualDensity: VisualDensity.compact,
          onPressed: () => controller.toggleStar(song.id),
          icon: Icon(
            starred ? Icons.favorite_rounded : Icons.favorite_border_rounded,
            color: starred ? kPink : Colors.white54,
            size: 20,
          ),
        ),
      ],
    );
  }
}

class _BarCenter extends StatelessWidget {
  const _BarCenter({required this.controller});
  final WaveController controller;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            _ToggleButton(
              icon: Icons.shuffle_rounded,
              active: controller.shuffle,
              onPressed: controller.toggleShuffle,
            ),
            IconButton(
              onPressed: controller.previous,
              icon: const Icon(Icons.skip_previous_rounded, size: 26),
              color: kCream,
            ),
            Container(
              margin: const EdgeInsets.symmetric(horizontal: 4),
              child: IconButton(
                onPressed: controller.toggle,
                iconSize: 22,
                style: IconButton.styleFrom(
                  backgroundColor: kCream,
                  foregroundColor: kBlack,
                  fixedSize: const Size(40, 40),
                ),
                icon: PlayPauseIcon(playing: controller.isPlaying, size: 22),
              ),
            ),
            IconButton(
              onPressed: () => controller.next(),
              icon: const Icon(Icons.skip_next_rounded, size: 26),
              color: kCream,
            ),
            _ToggleButton(
              icon: Icons.repeat_one_rounded,
              active: controller.repeatOne,
              onPressed: controller.toggleRepeat,
            ),
          ],
        ),
        _SeekBar(controller: controller),
      ],
    );
  }
}

class _ToggleButton extends StatelessWidget {
  const _ToggleButton({required this.icon, required this.active, required this.onPressed});
  final IconData icon;
  final bool active;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      visualDensity: VisualDensity.compact,
      onPressed: onPressed,
      icon: Icon(icon, size: 20, color: active ? kPink : Colors.white54),
    );
  }
}

class _SeekBar extends StatelessWidget {
  const _SeekBar({required this.controller});
  final WaveController controller;

  @override
  Widget build(BuildContext context) {
    final player = controller.player;
    final song = controller.current;
    return StreamBuilder<Duration>(
      stream: player.positionStream,
      initialData: player.position,
      builder: (context, snap) {
        final pos = snap.data ?? Duration.zero;
        final dur = player.duration ?? Duration(seconds: song?.duration ?? 1);
        final maxMs = dur.inMilliseconds.toDouble().clamp(1.0, double.infinity);
        return Row(
          children: [
            SizedBox(
              width: 40,
              child: Text(formatDuration(pos), textAlign: TextAlign.right,
                  style: const TextStyle(color: Colors.white54, fontSize: 11, fontWeight: FontWeight.w700)),
            ),
            Expanded(
              child: SliderTheme(
                data: _trackTheme(context),
                child: Slider(
                  min: 0,
                  max: maxMs,
                  value: pos.inMilliseconds.toDouble().clamp(0.0, maxMs),
                  onChanged: (v) => player.seek(Duration(milliseconds: v.round())),
                ),
              ),
            ),
            SizedBox(
              width: 40,
              child: Text(formatDuration(dur),
                  style: const TextStyle(color: Colors.white54, fontSize: 11, fontWeight: FontWeight.w700)),
            ),
          ],
        );
      },
    );
  }
}

class _BarRight extends StatelessWidget {
  const _BarRight({required this.controller});
  final WaveController controller;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.end,
      children: [
        // Decorative live visualizer (matches the reference player).
        SizedBox(
          height: 18,
          child: EqualizerBars(playing: controller.isPlaying, height: 18, color: kPink, barCount: 5),
        ),
        const SizedBox(width: 14),
        Icon(
          controller.volume <= 0.01
              ? Icons.volume_off_rounded
              : controller.volume < 0.5
                  ? Icons.volume_down_rounded
                  : Icons.volume_up_rounded,
          color: Colors.white54,
          size: 20,
        ),
        SizedBox(
          width: 110,
          child: SliderTheme(
            data: _trackTheme(context),
            child: Slider(
              min: 0,
              max: 1,
              value: controller.volume,
              onChanged: controller.setVolume,
            ),
          ),
        ),
      ],
    );
  }
}

SliderThemeData _trackTheme(BuildContext context) {
  return SliderTheme.of(context).copyWith(
    trackHeight: 3,
    activeTrackColor: kPink,
    inactiveTrackColor: kCream.withValues(alpha: 0.16),
    thumbColor: kCream,
    thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 5),
    overlayShape: const RoundSliderOverlayShape(overlayRadius: 12),
  );
}

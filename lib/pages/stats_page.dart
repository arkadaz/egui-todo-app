import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';
import '../widgets/live_builder.dart';

/// The desktop app's "Your Stats" window, plus a streak and a study chart.
/// Updates live while the timer runs.
class StatsPage extends StatefulWidget {
  const StatsPage({super.key, required this.controller});

  final FocusController controller;

  @override
  State<StatsPage> createState() => _StatsPageState();
}

class _StatsPageState extends State<StatsPage> {
  int _chartDays = 7;

  FocusController get _controller => widget.controller;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Your Stats')),
      // Live while on screen: study time counts up while a study session runs.
      body: LiveBuilder(
        controller: _controller,
        builder: (context) {
          final stats = _controller.stats;
          final chart = _controller.studyChart(_chartDays);
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              _StreakCard(stats: stats),
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(Icons.bar_chart, color: theme.colorScheme.primary),
                          const SizedBox(width: 8),
                          Expanded(child: Text('Study time', style: theme.textTheme.titleMedium)),
                          SegmentedButton<int>(
                            showSelectedIcon: false,
                            segments: const [
                              ButtonSegment(value: 7, label: Text('7 days')),
                              ButtonSegment(value: 30, label: Text('30 days')),
                            ],
                            selected: {_chartDays},
                            onSelectionChanged: (s) => setState(() => _chartDays = s.first),
                          ),
                        ],
                      ),
                      const SizedBox(height: 16),
                      StudyChart(days: chart),
                      const SizedBox(height: 12),
                      _Row('Last 7 days', stats.last7DaysTime),
                      if (stats.bestDay != null) _Row('Best day', stats.bestDay!),
                    ],
                  ),
                ),
              ),
              _StatCard(icon: Icons.insights, title: 'Lifetime Summary', rows: [('Total study time', stats.totalTime)]),
              _StatCard(
                icon: Icons.today,
                title: "Today's Progress",
                rows: [('Sessions completed', '${stats.todaySessions}'), ('Time studied', stats.todayTime)],
              ),
              _StatCard(
                icon: Icons.calendar_month,
                title: "This Month's Progress (${stats.monthName})",
                rows: [('Sessions completed', '${stats.monthSessions}')],
              ),
              Padding(
                padding: const EdgeInsets.all(8),
                child: Text(
                  'Study time counts while a study session runs. A session is completed when its '
                  'break ends. A day counts toward your streak after a minute of study.',
                  style: theme.textTheme.bodySmall,
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _StreakCard extends StatelessWidget {
  const _StreakCard({required this.stats});

  final StatsView stats;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final days = stats.streakDays;
    return Card(
      color: theme.colorScheme.primaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Text(days > 0 ? '🔥' : '🌱', style: const TextStyle(fontSize: 40)),
            const SizedBox(width: 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    days > 0 ? '$days-day streak' : 'No streak yet',
                    style: theme.textTheme.headlineSmall?.copyWith(color: theme.colorScheme.onPrimaryContainer),
                  ),
                  Text(
                    days > 0
                        ? 'Best: ${stats.bestStreakDays} ${stats.bestStreakDays == 1 ? 'day' : 'days'}'
                        : 'Study for a minute today to start one.',
                    style: theme.textTheme.bodyMedium?.copyWith(color: theme.colorScheme.onPrimaryContainer),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Bars of study minutes per day (Rust works out the numbers and labels).
/// Tap a bar to see its exact time.
class StudyChart extends StatelessWidget {
  const StudyChart({super.key, required this.days});

  final List<ChartDay> days;

  static const _height = 140.0;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final most = days.fold<double>(0, (m, d) => d.minutes > m ? d.minutes : m);
    final scale = most > 0 ? most : 1.0;
    final showEvery = days.length <= 7 ? 1 : 5; // label every 5th day on the 30-day chart
    return Column(
      children: [
        SizedBox(
          height: _height,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              for (final day in days)
                Expanded(
                  child: Tooltip(
                    triggerMode: TooltipTriggerMode.tap,
                    message: '${day.date}: ${_minutes(day.minutes)}',
                    child: Padding(
                      padding: EdgeInsets.symmetric(horizontal: days.length <= 7 ? 6 : 1.5),
                      child: Semantics(
                        label: '${day.date}, ${_minutes(day.minutes)}',
                        child: Container(
                          height: day.minutes <= 0 ? 2 : (_height - 4) * day.minutes / scale + 4,
                          decoration: BoxDecoration(
                            color: day.minutes <= 0
                                ? colors.outlineVariant
                                : (day.isToday ? colors.primary : colors.primary.withValues(alpha: 0.6)),
                            borderRadius: const BorderRadius.vertical(top: Radius.circular(4)),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 4),
        Row(
          children: [
            for (var i = 0; i < days.length; i++)
              Expanded(
                child: Text(
                  i % showEvery == 0 || days[i].isToday ? days[i].label : '',
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.visible,
                  style: Theme.of(context).textTheme.labelSmall
                      ?.copyWith(fontWeight: days[i].isToday ? FontWeight.bold : null),
                ),
              ),
          ],
        ),
        Align(
          alignment: Alignment.centerRight,
          child: Text(
            most > 0 ? 'Top bar: ${_minutes(most)}' : 'No study yet in this period',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ),
      ],
    );
  }

  static String _minutes(double minutes) {
    final whole = minutes.round();
    return whole >= 60 ? '${whole ~/ 60}h ${whole % 60}m' : '${whole}m';
  }
}

class _Row extends StatelessWidget {
  const _Row(this.label, this.value);

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Expanded(child: Text(label)),
          Text(
            value,
            style: Theme.of(context).textTheme.titleMedium
                ?.copyWith(fontFeatures: const [FontFeature.tabularFigures()]),
          ),
        ],
      ),
    );
  }
}

class _StatCard extends StatelessWidget {
  const _StatCard({required this.icon, required this.title, required this.rows});

  final IconData icon;
  final String title;
  final List<(String, String)> rows;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(icon, color: theme.colorScheme.primary),
                const SizedBox(width: 8),
                Expanded(child: Text(title, style: theme.textTheme.titleMedium)),
              ],
            ),
            const SizedBox(height: 8),
            for (final (label, value) in rows) _Row(label, value),
          ],
        ),
      ),
    );
  }
}

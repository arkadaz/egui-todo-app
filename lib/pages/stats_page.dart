import 'package:flutter/material.dart';

import '../controller.dart';

/// The desktop app's "Your Stats" window. Updates live while the timer runs.
class StatsPage extends StatelessWidget {
  const StatsPage({super.key, required this.controller});

  final FocusController controller;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Your Stats')),
      body: ListenableBuilder(
        listenable: Listenable.merge([controller, controller.ticks]),
        builder: (context, _) {
          final stats = controller.stats;
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              _StatCard(
                icon: Icons.insights,
                title: 'Lifetime Summary',
                rows: [('Total study time', stats.totalTime)],
              ),
              _StatCard(
                icon: Icons.today,
                title: "Today's Progress",
                rows: [
                  ('Sessions completed', '${stats.todaySessions}'),
                  ('Time studied', stats.todayTime),
                ],
              ),
              _StatCard(
                icon: Icons.calendar_month,
                title: "This Month's Progress (${stats.monthName})",
                rows: [('Sessions completed', '${stats.monthSessions}')],
              ),
              Padding(
                padding: const EdgeInsets.all(8),
                child: Text(
                  'Study time counts while a study session runs. '
                  'A session is completed when its break ends.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
            ],
          );
        },
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
            for (final (label, value) in rows)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Row(
                  children: [
                    Expanded(child: Text(label)),
                    Text(
                      value,
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontFeatures: const [FontFeature.tabularFigures()],
                      ),
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

import 'dart:convert';
import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import '../controller.dart';
import '../services/alerts.dart';

Future<void> showSettingsSheet(BuildContext context, FocusController controller) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (context) => DraggableScrollableSheet(
      expand: false,
      initialChildSize: 0.75,
      maxChildSize: 0.95,
      builder: (context, scroll) => SettingsSheet(controller: controller, scrollController: scroll),
    ),
  );
}

/// The desktop app's File and Settings menus: time zone, background, alerts and data.
class SettingsSheet extends StatefulWidget {
  const SettingsSheet({super.key, required this.controller, required this.scrollController});

  final FocusController controller;
  final ScrollController scrollController;

  @override
  State<SettingsSheet> createState() => _SettingsSheetState();
}

class _SettingsSheetState extends State<SettingsSheet> {
  late Future<AlertStatus> _status;
  late final AppLifecycleListener _lifecycle;

  FocusController get _controller => widget.controller;

  @override
  void initState() {
    super.initState();
    _status = _controller.alerts.status();
    // Coming back from the system settings screen: check the permissions again.
    _lifecycle = AppLifecycleListener(onResume: _refreshStatus);
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  void _refreshStatus() => setState(() => _status = _controller.alerts.status());

  void _say(String message) {
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), behavior: SnackBarBehavior.floating),
    );
  }

  Future<void> _pickTimeZone() async {
    final current = _controller.timeZone;
    // Open the list scrolled to the current choice (row 0 is "same as device", then -12..14).
    final row = current.followsDevice ? 0 : current.offsetHours + 13;
    final scroll = ScrollController(initialScrollOffset: (row - 3).clamp(0, 30) * 56.0);
    final choice = await showDialog<int?>(
      context: context,
      builder: (context) => SimpleDialog(
        title: const Text('Time zone for the clock'),
        children: [
          SizedBox(
            width: double.maxFinite,
            height: 420,
            child: ListView(
              controller: scroll,
              children: [
                _ZoneTile(
                  label: 'Same as this device',
                  selected: current.followsDevice,
                  onTap: () => Navigator.pop(context, _deviceZone),
                ),
                for (var hours = -12; hours <= 14; hours++)
                  _ZoneTile(
                    label: 'GMT${hours < 0 ? '' : '+'}$hours',
                    selected: !current.followsDevice && current.offsetHours == hours,
                    onTap: () => Navigator.pop(context, hours),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
    scroll.dispose();
    if (choice == null) return; // dialog dismissed
    _controller.setTimeZone(choice == _deviceZone ? null : choice);
  }

  Future<void> _chooseBackground() async {
    final file = await FilePicker.pickFile(
      dialogTitle: 'Choose a background',
      type: FileType.custom,
      allowedExtensions: const ['gif', 'png', 'jpg', 'jpeg', 'webp'],
    );
    if (file == null) return;
    final error = _controller.setBackground(file.name, await file.readAsBytes());
    if (!mounted) return;
    _say(error ?? 'Background changed.');
  }

  Future<void> _import() async {
    final file = await FilePicker.pickFile(dialogTitle: 'Choose focushub_data.json');
    if (file == null || !mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Replace your data?'),
        content: Text(
          'Your tasks, stats and rewards will be replaced by the ones in "${file.name}". '
          'Your timer settings stay the same.\n\nExport a backup first if you want to keep the current data.',
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancel')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Replace')),
        ],
      ),
    );
    if (confirmed != true) return;
    final text = utf8.decode(await file.readAsBytes(), allowMalformed: true);
    try {
      final summary = _controller.importJson(text);
      _say('Imported ${_count(summary.tasks, 'task')} on ${_count(summary.days, 'day')}, '
          'and ${_count(summary.rewards, 'reward')}.');
    } on AnyhowException catch (e) {
      _say(e.message);
    }
  }

  Future<void> _export() async {
    final json = _controller.exportJson();
    if (json == null) return;
    final saved = await FilePicker.saveFile(
      dialogTitle: 'Save a backup',
      fileName: 'focushub_data.json',
      bytes: Uint8List.fromList(utf8.encode(json)),
      mimeType: 'application/json',
    );
    if (!mounted) return;
    if (saved != null) _say('Backup saved.');
  }

  @override
  Widget build(BuildContext context) {
    final alerts = _controller.alerts;
    return ListenableBuilder(
      listenable: _controller,
      builder: (context, _) {
        return ListView(
          controller: widget.scrollController,
          padding: const EdgeInsets.only(bottom: 24),
          children: [
            const _Header('Clock'),
            ListTile(
              leading: const Icon(Icons.public),
              title: const Text('Time zone'),
              subtitle: Text(_controller.timeZone.label),
              onTap: _pickTimeZone,
            ),
            const _Header('Background'),
            ListTile(
              leading: const Icon(Icons.image_outlined),
              title: const Text('Choose an image or GIF…'),
              onTap: _chooseBackground,
            ),
            if (_controller.backgroundPath != null)
              ListTile(
                leading: const Icon(Icons.restore),
                title: const Text('Use the default background'),
                onTap: () {
                  _controller.clearBackground();
                  _say('Default background restored.');
                },
              ),
            if (alerts.supported) ...[
              const _Header('Alerts'),
              FutureBuilder<AlertStatus>(
                future: _status,
                builder: (context, snapshot) {
                  final status = snapshot.data;
                  if (status == null) return const SizedBox(height: 112);
                  return Column(
                    children: [
                      ListTile(
                        leading: const Icon(Icons.notifications_outlined),
                        title: const Text('Notifications'),
                        subtitle: Text(status.notifications ? 'On' : 'Off. Tap to allow.'),
                        trailing: Icon(status.notifications ? Icons.check_circle : Icons.error_outline),
                        onTap: status.notifications
                            ? null
                            : () async {
                                await alerts.askPermission();
                                _refreshStatus();
                              },
                      ),
                      ListTile(
                        leading: const Icon(Icons.alarm),
                        title: const Text('On-time alerts'),
                        subtitle: Text(
                          status.onTime
                              ? 'Alerts arrive exactly when a session ends.'
                              : 'Alerts may be a little late. Tap to allow "Alarms & reminders".',
                        ),
                        trailing: Icon(status.onTime ? Icons.check_circle : Icons.error_outline),
                        onTap: status.onTime ? null : alerts.askOnTimePermission,
                      ),
                    ],
                  );
                },
              ),
              ListTile(
                leading: const Icon(Icons.volume_up_outlined),
                title: const Text('Test the alert'),
                subtitle: const Text('Show a notification with the session-end beep now.'),
                onTap: alerts.showTest,
              ),
            ],
            const _Header('Your data'),
            ListTile(
              leading: const Icon(Icons.download_outlined),
              title: const Text('Import from the desktop app…'),
              subtitle: const Text('Use the tasks, stats and rewards from a focushub_data.json file.'),
              onTap: _import,
            ),
            ListTile(
              leading: const Icon(Icons.upload_outlined),
              title: const Text('Export a backup…'),
              subtitle: const Text('Save focushub_data.json. The desktop app can open it too.'),
              onTap: _export,
            ),
            ListTile(
              leading: const Icon(Icons.folder_outlined),
              title: const Text('Data file'),
              subtitle: Text(_controller.hub.dataFile()),
            ),
          ],
        );
      },
    );
  }
}

/// "1 task", "2 tasks"
String _count(int n, String word) => '$n $word${n == 1 ? '' : 's'}';

/// Stands for "follow the device" in the time zone dialog (a real offset is -12 to 14).
const _deviceZone = 1000;

class _ZoneTile extends StatelessWidget {
  const _ZoneTile({required this.label, required this.selected, required this.onTap});

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      title: Text(label),
      trailing: selected ? const Icon(Icons.check) : null,
      selected: selected,
      onTap: onTap,
    );
  }
}

class _Header extends StatelessWidget {
  const _Header(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
      child: Text(
        text,
        style: Theme.of(context).textTheme.titleSmall?.copyWith(color: Theme.of(context).colorScheme.primary),
      ),
    );
  }
}

import 'package:flutter/material.dart';

import '../controller.dart';
import '../widgets/edit_helpers.dart';

/// The desktop app's Rewards window. Unfinished rewards are listed first.
/// Tap a reward to rename it; deleting can be undone.
class RewardsPage extends StatefulWidget {
  const RewardsPage({super.key, required this.controller});

  final FocusController controller;

  @override
  State<RewardsPage> createState() => _RewardsPageState();
}

class _RewardsPageState extends State<RewardsPage> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  String? _error;

  FocusController get _controller => widget.controller;

  void _add() {
    final error = _controller.addReward(_input.text);
    setState(() => _error = error);
    if (error == null) {
      _input.clear();
      _focus.requestFocus();
    }
  }

  @override
  void dispose() {
    _input.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Rewards')),
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) {
          final rewards = _controller.rewards;
          return ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
            children: [
              Text('Add a New Reward', style: theme.textTheme.titleLarge),
              const SizedBox(height: 12),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: TextField(
                      controller: _input,
                      focusNode: _focus,
                      textInputAction: TextInputAction.done,
                      textCapitalization: TextCapitalization.sentences,
                      onTapOutside: (_) => _focus.unfocus(), // tap elsewhere to close the keyboard
                      onEditingComplete: _add,
                      decoration: InputDecoration(
                        hintText: 'Enter a new reward...',
                        border: const OutlineInputBorder(),
                        errorText: _error,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: IconButton.filled(
                      tooltip: 'Add reward',
                      icon: const Icon(Icons.add),
                      onPressed: _add,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 24),
              Text('Your Rewards', style: theme.textTheme.titleLarge),
              const Divider(),
              if (rewards.isEmpty)
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 12),
                  child: Text('No rewards yet. Add something to look forward to!'),
                )
              else
                Card(
                  child: Column(
                    children: [
                      for (final reward in rewards)
                        CheckboxListTile(
                          value: reward.completed,
                          controlAffinity: ListTileControlAffinity.leading,
                          onChanged: (done) => _controller.setRewardCompleted(reward.index, done ?? false),
                          title: Text(
                            reward.name,
                            style: reward.completed
                                ? TextStyle(
                                    decoration: TextDecoration.lineThrough,
                                    color: theme.colorScheme.onSurfaceVariant,
                                  )
                                : null,
                          ),
                          secondary: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              IconButton(
                                tooltip: 'Rename reward',
                                icon: const Icon(Icons.edit_outlined),
                                onPressed: () => showEditDialog(
                                  context,
                                  title: 'Rename reward',
                                  initial: reward.name,
                                  save: (name) => _controller.editReward(reward.index, name),
                                ),
                              ),
                              IconButton(
                                tooltip: 'Remove reward',
                                icon: const Icon(Icons.close),
                                onPressed: () =>
                                    showUndo(context, 'Reward deleted', _controller.deleteReward(reward.index)),
                              ),
                            ],
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

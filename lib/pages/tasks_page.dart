import 'package:flutter/material.dart';

import '../controller.dart';
import '../widgets/calendar_card.dart';

/// The desktop app's To-Do List and Calendar windows, together on one screen.
class TasksPage extends StatefulWidget {
  const TasksPage({super.key, required this.controller});

  final FocusController controller;

  @override
  State<TasksPage> createState() => _TasksPageState();
}

class _TasksPageState extends State<TasksPage> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  String? _error;

  FocusController get _controller => widget.controller;

  void _add() {
    final error = _controller.addTodo(_input.text);
    setState(() => _error = error);
    if (error == null) {
      _input.clear();
      _focus.requestFocus(); // keep typing the next task, like the desktop app
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
      appBar: AppBar(
        title: const Text('Tasks'),
        actions: [
          IconButton(
            tooltip: 'Go to today',
            icon: const Icon(Icons.today),
            onPressed: _controller.goToToday,
          ),
        ],
      ),
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) {
          final todos = _controller.todos;
          final history = _controller.history;
          return ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
            children: [
              CalendarCard(controller: _controller),
              const SizedBox(height: 16),
              Text('Task Editor', style: theme.textTheme.titleLarge),
              Text(_controller.selectedDateTitle, style: theme.textTheme.bodyMedium),
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
                      onEditingComplete: _add, // Enter adds the task and keeps the keyboard open
                      decoration: InputDecoration(
                        hintText: 'What needs to be done?',
                        border: const OutlineInputBorder(),
                        errorText: _error,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: IconButton.filled(
                      tooltip: 'Add task',
                      icon: const Icon(Icons.add),
                      onPressed: _add,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              if (todos.isEmpty)
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 12),
                  child: Text('No tasks for this day.'),
                )
              else
                Card(
                  child: Column(
                    children: [
                      for (final todo in todos)
                        CheckboxListTile(
                          value: todo.completed,
                          controlAffinity: ListTileControlAffinity.leading,
                          onChanged: (done) => _controller.setTodoCompleted(
                            _controller.selectedDate,
                            todo.index,
                            done ?? false,
                          ),
                          title: _TaskText(text: todo.text, done: todo.completed),
                          secondary: IconButton(
                            tooltip: 'Remove task',
                            icon: const Icon(Icons.close),
                            onPressed: () => _controller.deleteTodo(todo.index),
                          ),
                        ),
                    ],
                  ),
                ),
              const SizedBox(height: 24),
              Text('Task History', style: theme.textTheme.titleLarge),
              const Divider(),
              if (history.isEmpty)
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 12),
                  child: Text('No tasks from previous days.'),
                )
              else
                for (final day in history) ...[
                  Padding(
                    padding: const EdgeInsets.fromLTRB(4, 12, 4, 4),
                    child: Text(
                      day.title,
                      style: theme.textTheme.titleSmall?.copyWith(fontWeight: FontWeight.bold),
                    ),
                  ),
                  Card(
                    child: Column(
                      children: [
                        for (final todo in day.todos)
                          CheckboxListTile(
                            dense: true,
                            value: todo.completed,
                            controlAffinity: ListTileControlAffinity.leading,
                            onChanged: (done) =>
                                _controller.setTodoCompleted(day.date, todo.index, done ?? false),
                            title: _TaskText(text: todo.text, done: todo.completed),
                          ),
                      ],
                    ),
                  ),
                ],
            ],
          );
        },
      ),
    );
  }
}

class _TaskText extends StatelessWidget {
  const _TaskText({required this.text, required this.done});

  final String text;
  final bool done;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: done
          ? TextStyle(
              decoration: TextDecoration.lineThrough,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            )
          : null,
    );
  }
}

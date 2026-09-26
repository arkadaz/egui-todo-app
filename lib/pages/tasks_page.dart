import 'package:flutter/material.dart';

import '../controller.dart';
import '../src/rust/api/focus_hub.dart';
import '../widgets/calendar_card.dart';
import '../widgets/edit_helpers.dart';
import '../widgets/task_history.dart';

/// The desktop app's To-Do List and Calendar windows, together on one screen.
/// Tap a task to edit it, drag the handle to reorder, delete with Undo. Earlier days are
/// grouped by month and can be filtered and searched, so a long history stays easy to use.
class TasksPage extends StatefulWidget {
  const TasksPage({super.key, required this.controller});

  final FocusController controller;

  @override
  State<TasksPage> createState() => _TasksPageState();
}

class _TasksPageState extends State<TasksPage> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  final _scroll = ScrollController();
  final _searchInput = TextEditingController();
  String? _error;

  HistoryFilter _filter = HistoryFilter.all;
  bool _searching = false;

  /// Months whose open/closed state the user flipped. The newest month starts open,
  /// the others closed.
  final _flipped = <String>{};

  /// Finished days the user opened (finished days show just one line at first).
  final _openDays = <String>{};

  FocusController get _controller => widget.controller;

  void _add() {
    final error = _controller.addTodo(_input.text);
    setState(() => _error = error);
    if (error == null) {
      _input.clear();
      _focus.requestFocus(); // keep typing the next task, like the desktop app
    }
  }

  void _edit(String date, TodoView todo) {
    showEditDialog(
      context,
      title: 'Edit task',
      initial: todo.text,
      save: (text) => _controller.editTodo(date, todo.index, text),
    );
  }

  void _delete(TodoView todo) {
    final undo = _controller.deleteTodo(todo.index);
    showUndo(context, 'Task deleted', undo);
  }

  void _moveUnfinished() {
    final moved = _controller.moveUnfinishedToToday();
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        behavior: SnackBarBehavior.floating,
        content: Text('Moved $moved unfinished ${moved == 1 ? 'task' : 'tasks'} to today.'),
      ),
    );
  }

  /// Shows an earlier day in the editor at the top.
  void _open(String date) {
    _controller.selectDate(date);
    _scroll.animateTo(0, duration: const Duration(milliseconds: 300), curve: Curves.easeOut);
  }

  void _toggleSearch() {
    setState(() {
      _searching = !_searching;
      if (!_searching) _searchInput.clear();
    });
  }

  bool _isOpen(HistoryMonth month, int position) {
    if (_searchInput.text.trim().isNotEmpty) return true; // show every match
    return (position == 0) != _flipped.contains(month.key);
  }

  bool _showsTasks(DayTodos day) =>
      !day.allDone ||
      _filter == HistoryFilter.done ||
      _searchInput.text.trim().isNotEmpty ||
      _openDays.contains(day.date);

  void _flipDay(DayTodos day) {
    setState(() => _openDays.contains(day.date) ? _openDays.remove(day.date) : _openDays.add(day.date));
  }

  void _flip(HistoryMonth month) {
    setState(() => _flipped.contains(month.key) ? _flipped.remove(month.key) : _flipped.add(month.key));
  }

  String _emptyHistoryText() {
    final search = _searchInput.text.trim();
    if (search.isNotEmpty) return 'No earlier tasks match "$search".';
    return switch (_filter) {
      HistoryFilter.all => 'No tasks from previous days.',
      HistoryFilter.unfinished => 'Nothing unfinished from earlier days.',
      HistoryFilter.done => 'No finished tasks from earlier days yet.',
    };
  }

  @override
  void dispose() {
    _input.dispose();
    _focus.dispose();
    _scroll.dispose();
    _searchInput.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Tasks'),
        actions: [IconButton(tooltip: 'Go to today', icon: const Icon(Icons.today), onPressed: _controller.goToToday)],
      ),
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) {
          final date = _controller.selectedDate;
          final todos = _controller.todos;
          final months = _controller.history(filter: _filter, search: _searchInput.text);
          final unfinished = _controller.isTodaySelected ? _controller.unfinishedBeforeToday : 0;
          // One row per month header, plus one per day of the open months. Built lazily.
          final rows = <Object>[
            for (final (i, month) in months.indexed) ...[
              (month, _isOpen(month, i)),
              if (_isOpen(month, i)) ...month.days,
            ],
          ];
          const side = EdgeInsets.symmetric(horizontal: 16);
          return CustomScrollView(
            controller: _scroll,
            slivers: [
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                sliver: SliverList.list(
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
                          child: IconButton.filled(tooltip: 'Add task', icon: const Icon(Icons.add), onPressed: _add),
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    if (todos.isEmpty)
                      const Padding(padding: EdgeInsets.symmetric(vertical: 12), child: Text('No tasks for this day.')),
                  ],
                ),
              ),
              if (todos.isNotEmpty)
                SliverPadding(
                  padding: side,
                  sliver: SliverReorderableList(
                    itemCount: todos.length,
                    onReorderItem: _controller.moveTodo, // gives the final position
                    proxyDecorator: (child, index, animation) => Material(
                      elevation: 6,
                      borderRadius: BorderRadius.circular(12),
                      color: theme.colorScheme.surfaceContainerHighest,
                      child: child,
                    ),
                    itemBuilder: (context, i) {
                      final todo = todos[i];
                      return _TaskTile(
                        key: ValueKey('$date/${todo.index}/${todo.text}'),
                        todo: todo,
                        index: i,
                        onChanged: (done) => _controller.setTodoCompleted(date, todo.index, done),
                        onEdit: () => _edit(date, todo),
                        onDelete: () => _delete(todo),
                      );
                    },
                  ),
                ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(16, 24, 16, 0),
                sliver: SliverList.list(
                  children: [
                    Row(
                      children: [
                        Expanded(child: Text('Task History', style: theme.textTheme.titleLarge)),
                        IconButton(
                          tooltip: _searching ? 'Stop searching' : 'Search earlier tasks',
                          icon: Icon(_searching ? Icons.search_off : Icons.search),
                          onPressed: _toggleSearch,
                        ),
                      ],
                    ),
                    if (_searching)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 8),
                        child: TextField(
                          controller: _searchInput,
                          autofocus: true,
                          onChanged: (_) => setState(() {}),
                          onTapOutside: (_) => FocusScope.of(context).unfocus(),
                          decoration: InputDecoration(
                            hintText: 'Search earlier tasks',
                            prefixIcon: const Icon(Icons.search),
                            border: const OutlineInputBorder(),
                            isDense: true,
                            suffixIcon: _searchInput.text.isEmpty
                                ? null
                                : IconButton(
                                    tooltip: 'Clear',
                                    icon: const Icon(Icons.clear),
                                    onPressed: () => setState(_searchInput.clear),
                                  ),
                          ),
                        ),
                      ),
                    SegmentedButton<HistoryFilter>(
                      showSelectedIcon: false,
                      segments: const [
                        ButtonSegment(value: HistoryFilter.all, label: Text('All')),
                        ButtonSegment(value: HistoryFilter.unfinished, label: Text('Unfinished')),
                        ButtonSegment(value: HistoryFilter.done, label: Text('Done')),
                      ],
                      selected: {_filter},
                      onSelectionChanged: (choice) => setState(() => _filter = choice.first),
                    ),
                    const SizedBox(height: 8),
                    if (unfinished > 0)
                      Card(
                        color: theme.colorScheme.secondaryContainer,
                        child: ListTile(
                          leading: const Icon(Icons.move_down),
                          title: Text('$unfinished unfinished ${unfinished == 1 ? 'task' : 'tasks'} from earlier days'),
                          trailing: FilledButton.tonal(onPressed: _moveUnfinished, child: const Text('Move to today')),
                        ),
                      ),
                    if (months.isEmpty)
                      Padding(padding: const EdgeInsets.symmetric(vertical: 12), child: Text(_emptyHistoryText())),
                  ],
                ),
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 24),
                sliver: SliverList.builder(
                  itemCount: rows.length,
                  itemBuilder: (context, i) => switch (rows[i]) {
                    (final HistoryMonth month, final bool open) => MonthHeader(
                      month: month,
                      open: open,
                      onTap: () => _flip(month),
                    ),
                    final DayTodos day => HistoryDayCard(
                      day: day,
                      showTasks: _showsTasks(day),
                      onToggle: day.allDone ? () => _flipDay(day) : null,
                      onChanged: (index, done) => _controller.setTodoCompleted(day.date, index, done),
                      onOpen: () => _open(day.date),
                    ),
                    _ => const SizedBox.shrink(),
                  },
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

/// A task in the editor: checkbox, text (tap to edit), delete, and a drag handle.
class _TaskTile extends StatelessWidget {
  const _TaskTile({
    super.key,
    required this.todo,
    required this.index,
    required this.onChanged,
    required this.onEdit,
    required this.onDelete,
  });

  final TodoView todo;
  final int index;
  final ValueChanged<bool> onChanged;
  final VoidCallback onEdit;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    return Material(
      type: MaterialType.transparency,
      child: ListTile(
        contentPadding: const EdgeInsets.only(left: 4),
        leading: Checkbox(value: todo.completed, onChanged: (done) => onChanged(done ?? false)),
        title: TaskText(text: todo.text, done: todo.completed),
        onTap: onEdit,
        trailing: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            IconButton(tooltip: 'Remove task', icon: const Icon(Icons.close), onPressed: onDelete),
            ReorderableDragStartListener(
              index: index,
              child: const Padding(
                padding: EdgeInsets.all(12),
                child: Icon(Icons.drag_handle, semanticLabel: 'Drag to reorder'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

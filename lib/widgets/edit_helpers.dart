import 'package:flutter/material.dart';

/// A dialog for changing a task's or reward's text. [save] returns an error message
/// (shown under the text box), or `null` when it worked and the dialog can close.
Future<void> showEditDialog(
  BuildContext context, {
  required String title,
  required String initial,
  required String? Function(String text) save,
}) {
  return showDialog<void>(
    context: context,
    builder: (context) => _EditDialog(title: title, initial: initial, save: save),
  );
}

class _EditDialog extends StatefulWidget {
  const _EditDialog({required this.title, required this.initial, required this.save});

  final String title;
  final String initial;
  final String? Function(String text) save;

  @override
  State<_EditDialog> createState() => _EditDialogState();
}

class _EditDialogState extends State<_EditDialog> {
  late final _text = TextEditingController(text: widget.initial);
  String? _error;

  void _save() {
    final error = widget.save(_text.text);
    if (error == null) {
      Navigator.pop(context);
    } else {
      setState(() => _error = error);
    }
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.title),
      content: TextField(
        controller: _text,
        autofocus: true,
        textCapitalization: TextCapitalization.sentences,
        onSubmitted: (_) => _save(),
        decoration: InputDecoration(border: const OutlineInputBorder(), errorText: _error),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
        FilledButton(onPressed: _save, child: const Text('Save')),
      ],
    );
  }
}

/// "Task deleted  [Undo]" at the bottom of the screen.
void showUndo(BuildContext context, String message, VoidCallback? undo) {
  final messenger = ScaffoldMessenger.of(context);
  messenger.hideCurrentSnackBar();
  messenger.showSnackBar(
    SnackBar(
      content: Text(message),
      behavior: SnackBarBehavior.floating,
      duration: const Duration(seconds: 5),
      action: undo == null ? null : SnackBarAction(label: 'Undo', onPressed: undo),
    ),
  );
}

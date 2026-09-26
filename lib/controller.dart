import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'services/alerts.dart';
import 'src/rust/api/focus_hub.dart';

/// Connects the screens to the Rust `FocusHub`, which owns all the data and logic.
///
/// Screens read from this and call its methods. After a change it calls
/// `notifyListeners()`, which makes the screens that listen to it redraw.
class FocusController extends ChangeNotifier {
  FocusController({required this.hub, required this.alerts}) {
    _showDate(todayDate());
  }

  final FocusHub hub;
  final Alerts alerts;

  /// Counts timer ticks. The clock, timer and stats listen to this, so they
  /// redraw 4 times a second without rebuilding the other screens.
  final ticks = ValueNotifier<int>(0);

  final _sessionEnded = StreamController<SessionAlert>.broadcast();
  final _messages = StreamController<String>.broadcast();

  /// A study session or break just ended.
  Stream<SessionAlert> get sessionEnded => _sessionEnded.stream;

  /// Short messages (mostly errors) for the app to show.
  Stream<String> get messages => _messages.stream;

  Timer? _ticker;
  String _today = todayDate();

  // ---- App lifecycle --------------------------------------------------------------------

  /// The app is on screen: tick 4 times a second and refresh the alerts.
  void resume() {
    _ticker ??= Timer.periodic(const Duration(milliseconds: 250), (_) => _tick());
    // If the date changed while the app was away and "today" was selected, follow it.
    final today = todayDate();
    if (today != _today) {
      if (selectedDate == _today) _showDate(today);
      _today = today;
      notifyListeners();
    }
    _tick();
    unawaited(rescheduleAlerts());
  }

  /// The app is hidden: stop ticking and save. Scheduled notifications still fire.
  void pause() {
    _ticker?.cancel();
    _ticker = null;
    _run(hub.save);
  }

  void _tick() {
    final ended = _run(hub.tick) ?? const <SessionAlert>[];
    for (final alert in ended) {
      _sessionEnded.add(alert);
    }
    ticks.value++;
    if (ended.isNotEmpty) notifyListeners();
  }

  // ---- Clock and timer ------------------------------------------------------------------

  String get clock => hub.clockText();
  TimerView get timer => hub.timerView();

  Future<void> toggleTimer() async {
    final starting = !hub.timerView().isRunning;
    // The first start is a good moment to ask for notification permission.
    if (starting) await alerts.askPermission();
    final ended = _run(hub.toggleTimer) ?? const <SessionAlert>[];
    for (final alert in ended) {
      _sessionEnded.add(alert);
    }
    _changed();
    await rescheduleAlerts();
  }

  Future<void> resetTimer() async {
    _run(hub.resetTimer);
    _changed();
    await rescheduleAlerts();
  }

  Future<void> setTimerSettings({int? workSecs, int? breakSecs, int? loops}) async {
    final t = hub.timerView();
    _run(() => hub.setTimerSettings(
          workSecs: workSecs ?? t.workSecs,
          breakSecs: breakSecs ?? t.breakSecs,
          loops: loops ?? t.loops,
        ));
    _changed();
    await rescheduleAlerts();
  }

  /// Schedules a notification for every session end still to come, so they arrive on
  /// time even if the phone is asleep or the app is closed.
  Future<void> rescheduleAlerts() => alerts.schedule(hub.upcomingAlerts());

  TimeZoneView get timeZone => hub.timeZone();

  void setTimeZone(int? offsetHours) {
    _run(() => hub.setTimeZone(offsetHours: offsetHours));
    _changed();
  }

  // ---- Tasks and calendar ---------------------------------------------------------------

  late String selectedDate;
  late int calendarYear;
  late int calendarMonth;

  void _showDate(String date) {
    selectedDate = date;
    final month = monthOf(date: date);
    calendarYear = month.year;
    calendarMonth = month.month;
  }

  void selectDate(String date) {
    _showDate(date);
    notifyListeners();
  }

  void goToToday() => selectDate(todayDate());

  void showMonth(int delta) {
    final month = shiftMonth(year: calendarYear, month: calendarMonth, delta: delta);
    calendarYear = month.year;
    calendarMonth = month.month;
    notifyListeners();
  }

  CalendarMonth get calendar =>
      hub.calendarMonth(year: calendarYear, month: calendarMonth, selectedDate: selectedDate);

  String get selectedDateTitle => longDate(date: selectedDate);
  List<TodoView> get todos => hub.todosFor(date: selectedDate);
  List<DayTodos> get history => hub.historyBefore(date: selectedDate);

  /// Returns an error message, or `null` if the task was added.
  String? addTodo(String text) => _attempt(() => hub.addTodo(date: selectedDate, text: text));

  void setTodoCompleted(String date, int index, bool completed) =>
      _change(() => hub.setTodoCompleted(date: date, index: index, completed: completed));

  void deleteTodo(int index) => _change(() => hub.deleteTodo(date: selectedDate, index: index));

  // ---- Rewards --------------------------------------------------------------------------

  List<RewardView> get rewards => hub.rewards();

  /// Returns an error message, or `null` if the reward was added.
  String? addReward(String name) => _attempt(() => hub.addReward(name: name));

  void setRewardCompleted(int index, bool completed) =>
      _change(() => hub.setRewardCompleted(index: index, completed: completed));

  void deleteReward(int index) => _change(() => hub.deleteReward(index: index));

  // ---- Stats ----------------------------------------------------------------------------

  StatsView get stats => hub.stats();

  // ---- Background -----------------------------------------------------------------------

  String? get backgroundPath => hub.backgroundPath();

  /// Returns an error message, or `null` if the background was changed.
  String? setBackground(String fileName, Uint8List bytes) =>
      _attempt(() => hub.setBackground(fileName: fileName, bytes: bytes));

  void clearBackground() => _change(hub.clearBackground);

  // ---- Import / export ------------------------------------------------------------------

  String? exportJson() => _run(hub.exportJson);

  /// Returns a summary, or throws [AnyhowException] with a message for the user.
  ImportSummary importJson(String json) {
    final summary = hub.importJson(json: json);
    _changed();
    return summary;
  }

  // ---- Helpers --------------------------------------------------------------------------

  void _changed() {
    ticks.value++;
    notifyListeners();
  }

  /// Runs a change, then redraws. Errors are shown as messages.
  void _change(void Function() action) {
    _run(action);
    _changed();
  }

  /// Runs a change, then redraws. Returns the error message instead of showing it.
  String? _attempt(void Function() action) {
    try {
      action();
      _changed();
      return null;
    } on AnyhowException catch (e) {
      return e.message;
    }
  }

  /// Calls Rust, turning its errors into messages. Returns `null` on error.
  T? _run<T>(T Function() action) {
    try {
      return action();
    } on AnyhowException catch (e) {
      _messages.add(e.message);
      return null;
    }
  }

  @override
  void dispose() {
    _ticker?.cancel();
    ticks.dispose();
    _sessionEnded.close();
    _messages.close();
    super.dispose();
  }
}

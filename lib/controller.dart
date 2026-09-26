import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';

import 'services/alerts.dart';
import 'services/home_screen.dart';
import 'src/rust/api/focus_hub.dart';

/// Connects the screens to the Rust `FocusHub`, which owns all the data and logic.
///
/// Screens read from this and call its methods. After a change it calls
/// `notifyListeners()`, which makes the screens that listen to it redraw.
class FocusController extends ChangeNotifier {
  FocusController({required this.hub, required this.alerts, HomeScreen? homeScreen})
    : homeScreen = homeScreen ?? HomeScreen() {
    _showDate(todayDate());
  }

  final FocusHub hub;
  final Alerts alerts;
  final HomeScreen homeScreen;

  /// Changes whenever the clock or the countdown shows a new number (about once a second).
  /// The timer and stats listen to this, so they stay live without rebuilding other screens.
  final ticks = ValueNotifier<int>(0);

  /// Fires on any change: data ([notifyListeners]) or a new second ([ticks]).
  late final Listenable live = Listenable.merge([this, ticks]);

  final _sessionEnded = StreamController<SessionAlert>.broadcast();
  final _messages = StreamController<String>.broadcast();

  /// A study session or break just ended.
  Stream<SessionAlert> get sessionEnded => _sessionEnded.stream;

  /// Short messages (mostly errors) for the app to show.
  Stream<String> get messages => _messages.stream;

  Timer? _ticker;
  String _today = todayDate();

  // ---- App lifecycle --------------------------------------------------------------------

  /// The app is on screen: pick up changes made from notification buttons, start ticking,
  /// and bring the notifications up to date.
  void resume() {
    final changedElsewhere = _run(hub.reloadIfChanged) ?? false;
    // If the date changed while the app was away and "today" was selected, follow it.
    final today = todayDate();
    if (today != _today) {
      if (selectedDate == _today) _showDate(today);
      _today = today;
    }
    _tick();
    if (changedElsewhere) _changed();
    unawaited(syncAlerts());
  }

  /// The app is hidden: stop ticking and save. Nothing runs until it's back: scheduled
  /// notifications still fire, and Android itself counts down in the notification.
  ///
  /// This also runs on the way back (Flutter passes through "hidden"), so the save must not
  /// overwrite what a notification button did meanwhile: Rust loads that instead.
  void pause() {
    _ticker?.cancel();
    _ticker = null;
    if (_run(hub.save) ?? false) _changed();
  }

  /// Runs right after each change of a number on screen (Rust says when that is: about
  /// once a second), so the countdown is on time and the phone sleeps in between.
  void _tick() {
    _ticker?.cancel();
    final tick = _run(hub.tick);
    _ticker = Timer(Duration(milliseconds: tick?.nextTickMs ?? 1000), _tick);
    if (tick == null) return;
    _report(tick.ended);
    if (tick.ended.isNotEmpty || tick.reloaded) {
      _changed(); // a session ended or a notification button changed something: redraw all
    } else if (tick.redraw) {
      ticks.value++; // only the clock, timer and stats
    }
  }

  void _report(List<SessionAlert> ended) {
    for (final alert in ended) {
      _sessionEnded.add(alert);
    }
  }

  // ---- Clock and timer ------------------------------------------------------------------

  String get clock => hub.clockText();
  TimerView get timer => hub.timerView();
  List<PresetView> get timerPresets => presets();

  Future<void> toggleTimer() async {
    final starting = !hub.timerView().isRunning;
    // The first start is a good moment to ask for notification permission.
    if (starting) await alerts.askPermission();
    _report(_run(hub.toggleTimer) ?? const []);
    await _timerChanged();
  }

  Future<void> skipSession() async {
    _report(_run(hub.skipSession) ?? const []);
    await _timerChanged();
  }

  Future<void> resetTimer() async {
    _run(hub.resetTimer);
    await _timerChanged();
  }

  Future<void> setTimerSettings({
    int? workSecs,
    int? breakSecs,
    int? longBreakSecs,
    int? loops,
    int? longBreakEvery,
    bool? autoStart,
  }) async {
    final t = hub.timerView();
    _run(
      () => hub.setTimerSettings(
        workSecs: workSecs ?? t.workSecs,
        breakSecs: breakSecs ?? t.breakSecs,
        longBreakSecs: longBreakSecs ?? t.longBreakSecs,
        loops: loops ?? t.loops,
        longBreakEvery: longBreakEvery ?? t.longBreakEvery,
        autoStart: autoStart ?? t.autoStart,
      ),
    );
    await _timerChanged();
  }

  Future<void> applyPreset(int index) async {
    _run(() => hub.applyPreset(index: index));
    await _timerChanged();
  }

  Future<void> _timerChanged() async {
    _changed();
    await syncAlerts();
  }

  /// Updates the countdown notification and schedules every alert still to come, so they
  /// arrive on time even if the phone is asleep or the app is closed.
  Future<void> syncAlerts() => alerts.sync(hub);

  TimeZoneView get timeZone => hub.timeZone();

  void setTimeZone(int? offsetHours) {
    _run(() => hub.setTimeZone(offsetHours: offsetHours));
    _changed();
  }

  // ---- Tasks and calendar ---------------------------------------------------------------

  late String selectedDate;
  late int calendarYear;
  late int calendarMonth;

  /// The calendar shows the whole month, or only the selected day's week.
  bool calendarExpanded = false;

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

  bool get isTodaySelected => selectedDate == todayDate();

  void showMonth(int delta) {
    final month = shiftMonth(year: calendarYear, month: calendarMonth, delta: delta);
    calendarYear = month.year;
    calendarMonth = month.month;
    notifyListeners();
  }

  void toggleCalendar() {
    calendarExpanded = !calendarExpanded;
    _showDate(selectedDate); // the month shown is the selected day's
    notifyListeners();
  }

  /// Moves the selected day [weeks] weeks later (or earlier, if negative).
  void shiftWeek(int weeks) => selectDate(addDays(date: selectedDate, days: 7 * weeks));

  CalendarMonth get calendar => hub.calendarMonth(year: calendarYear, month: calendarMonth, selectedDate: selectedDate);

  CalendarWeek get calendarWeek => hub.calendarWeek(selectedDate: selectedDate);

  String get selectedDateTitle => longDate(date: selectedDate);
  List<TodoView> get todos => hub.todosFor(date: selectedDate);

  /// Earlier days' tasks, grouped by month. Rust does the filtering and searching.
  List<HistoryMonth> history({HistoryFilter filter = HistoryFilter.all, String search = ''}) =>
      _run(() => hub.history(before: selectedDate, filter: filter, search: search)) ?? const [];

  /// Returns an error message, or `null` if the task was added.
  String? addTodo(String text) => _attempt(() => hub.addTodo(date: selectedDate, text: text));

  /// Returns an error message, or `null` if the task was changed.
  String? editTodo(String date, int index, String text) =>
      _attempt(() => hub.editTodo(date: date, index: index, text: text));

  void setTodoCompleted(String date, int index, bool completed) =>
      _change(() => hub.setTodoCompleted(date: date, index: index, completed: completed));

  /// Deletes a task. Returns an "undo" function, or `null` if it failed.
  VoidCallback? deleteTodo(int index) {
    final date = selectedDate;
    final removed = _run(() => hub.deleteTodo(date: date, index: index));
    _changed();
    if (removed == null) return null;
    return () =>
        _change(() => hub.restoreTodo(date: date, index: index, text: removed.text, completed: removed.completed));
  }

  /// For drag-to-reorder: moves the task at [from] to position [to].
  void moveTodo(int from, int to) {
    if (from == to) return;
    _change(() => hub.moveTodo(date: selectedDate, from: from, to: to));
  }

  /// Unfinished tasks on days before today.
  int get unfinishedBeforeToday => _run(() => hub.unfinishedBefore(date: todayDate())) ?? 0;

  /// Moves every unfinished task from earlier days onto today. Returns how many moved.
  int moveUnfinishedToToday() {
    final moved = _run(() => hub.moveUnfinishedTo(date: todayDate())) ?? 0;
    _changed();
    return moved;
  }

  // ---- Rewards --------------------------------------------------------------------------

  List<RewardView> get rewards => hub.rewards();

  /// Returns an error message, or `null` if the reward was added.
  String? addReward(String name) => _attempt(() => hub.addReward(name: name));

  /// Returns an error message, or `null` if the reward was renamed.
  String? editReward(int index, String name) => _attempt(() => hub.editReward(index: index, name: name));

  void setRewardCompleted(int index, bool completed) =>
      _change(() => hub.setRewardCompleted(index: index, completed: completed));

  /// Deletes a reward. Returns an "undo" function, or `null` if it failed.
  VoidCallback? deleteReward(int index) {
    final removed = _run(() => hub.deleteReward(index: index));
    _changed();
    if (removed == null) return null;
    return () => _change(() => hub.restoreReward(index: index, name: removed.text, completed: removed.completed));
  }

  // ---- Stats ----------------------------------------------------------------------------

  StatsView get stats => hub.stats();

  List<ChartDay> studyChart(int days) => hub.studyChart(days: days);

  // ---- Background -----------------------------------------------------------------------

  String? get backgroundPath => hub.backgroundPath();

  /// Whether the background GIF plays. Off saves battery.
  bool get animateBackground => hub.animateBackground();

  void setAnimateBackground(bool animate) => _change(() => hub.setAnimateBackground(animate: animate));

  /// Makes the image at [path] the background. Rust reads it and shrinks it to fit a screen
  /// whose longest side is [screenSide] pixels, on a background thread (big photos and long
  /// GIFs take a moment), so the app never decodes more pixels than the screen can show.
  /// Returns the message to show: what changed, or what went wrong.
  Future<String> setBackground(String path, int screenSide) async {
    try {
      final image = await prepareBackground(path: path, screenSide: screenSide);
      final message = image.message();
      hub.setBackground(image: image); // hands the image over to Rust: no copy through Dart
      _changed();
      return message;
    } on AnyhowException catch (e) {
      return e.message;
    }
  }

  void clearBackground() => _change(hub.clearBackground);

  // ---- Home screen icon ------------------------------------------------------------------

  /// Offer the Home screen icon on first launch, if the launcher can add one.
  Future<bool> shouldOfferHomeIcon() async => !hub.homeIconOffered() && await homeScreen.canAdd();

  /// Remembers that the offer was made (whatever the answer), so it's made only once.
  void homeIconOffered() => _run(hub.setHomeIconOffered);

  // ---- Import / export / backups ----------------------------------------------------------

  String? exportJson() => _run(hub.exportJson);

  /// Replaces the data. Throws [AnyhowException] with a message for the user.
  ImportSummary importJson(String json) {
    final summary = hub.importJson(json: json);
    _changed();
    return summary;
  }

  /// Adds to the data. Throws [AnyhowException] with a message for the user.
  MergeSummary mergeJson(String json) {
    final summary = hub.mergeJson(json: json);
    _changed();
    return summary;
  }

  List<BackupView> get backups => hub.backups();

  /// Replaces everything with a daily backup (including the timer, so alerts are rescheduled).
  Future<void> restoreBackup(String date) async {
    _run(() => hub.restoreBackup(date: date));
    await _timerChanged();
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

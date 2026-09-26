//! Tasks: each day's to-do list, the history of earlier days, and the calendar marks.

use super::*;

impl Hub {
    pub fn todos(&self, date: NaiveDate) -> Vec<TodoItem> {
        self.data.todos_by_date.get(&date).cloned().unwrap_or_default()
    }

    pub fn add_todo(&mut self, date: NaiveDate, text: &str, now_ms: i64) -> Result<()> {
        let text = checked_text(text, "a task")?;
        self.reload_if_changed()?;
        self.data
            .todos_by_date
            .entry(date)
            .or_default()
            .push(TodoItem { text, completed: false });
        self.save(now_ms, [Change::Todos(date)])
    }

    pub fn set_todo_completed(&mut self, date: NaiveDate, index: usize, completed: bool, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        self.todo_mut(date, index)?.completed = completed;
        self.save(now_ms, [Change::Todos(date)])
    }

    pub fn edit_todo(&mut self, date: NaiveDate, index: usize, text: &str, now_ms: i64) -> Result<()> {
        let text = checked_text(text, "a task")?;
        self.reload_if_changed()?;
        self.todo_mut(date, index)?.text = text;
        self.save(now_ms, [Change::Todos(date)])
    }

    fn todo_mut(&mut self, date: NaiveDate, index: usize) -> Result<&mut TodoItem> {
        self.data
            .todos_by_date
            .get_mut(&date)
            .and_then(|todos| todos.get_mut(index))
            .context("That task no longer exists.")
    }

    /// Deletes a task and returns it (so it can be restored with `restore_todo`).
    pub fn delete_todo(&mut self, date: NaiveDate, index: usize, now_ms: i64) -> Result<TodoItem> {
        self.reload_if_changed()?;
        let todos = self
            .data
            .todos_by_date
            .get_mut(&date)
            .context("That task no longer exists.")?;
        if index >= todos.len() {
            bail!("That task no longer exists.");
        }
        let removed = todos.remove(index);
        if todos.is_empty() {
            self.data.todos_by_date.remove(&date);
        }
        self.save(now_ms, [Change::Todos(date)])?;
        Ok(removed)
    }

    /// Puts a deleted task back where it was (for "Undo").
    pub fn restore_todo(&mut self, date: NaiveDate, index: usize, todo: TodoItem, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        let todos = self.data.todos_by_date.entry(date).or_default();
        let index = index.min(todos.len());
        todos.insert(index, todo);
        self.save(now_ms, [Change::Todos(date)])
    }

    /// Moves a task within its day (for drag-to-reorder). `to` is its new position.
    pub fn move_todo(&mut self, date: NaiveDate, from: usize, to: usize, now_ms: i64) -> Result<()> {
        self.reload_if_changed()?;
        let todos = self
            .data
            .todos_by_date
            .get_mut(&date)
            .context("That task no longer exists.")?;
        if from >= todos.len() {
            bail!("That task no longer exists.");
        }
        let todo = todos.remove(from);
        let to = to.min(todos.len());
        todos.insert(to, todo);
        self.save(now_ms, [Change::Todos(date)])
    }

    /// Days before `date` with tasks that pass `filter` and contain `search` (ignoring
    /// case; empty matches everything), newest first.
    pub fn history(&self, date: NaiveDate, filter: HistoryFilter, search: &str) -> Vec<HistoryDay> {
        let search = search.trim().to_lowercase();
        let mut days: Vec<HistoryDay> = self
            .data
            .todos_by_date
            .iter()
            .filter(|(day, _)| **day < date)
            .filter_map(|(day, todos)| {
                let matching: Vec<(usize, TodoItem)> = todos
                    .iter()
                    .enumerate()
                    .filter(|(_, todo)| filter.accepts(todo))
                    .filter(|(_, todo)| search.is_empty() || todo.text.to_lowercase().contains(&search))
                    .map(|(index, todo)| (index, todo.clone()))
                    .collect();
                (!matching.is_empty()).then(|| HistoryDay {
                    date: *day,
                    todos: matching,
                    done: todos.iter().filter(|todo| todo.completed).count(),
                    total: todos.len(),
                })
            })
            .collect();
        days.sort_by_key(|day| std::cmp::Reverse(day.date));
        days
    }

    /// How many unfinished tasks there are on days before `date`.
    pub fn unfinished_before(&self, date: NaiveDate) -> u32 {
        self.data
            .todos_by_date
            .iter()
            .filter(|(day, _)| **day < date)
            .map(|(_, todos)| todos.iter().filter(|t| !t.completed).count() as u32)
            .sum()
    }

    /// Moves every unfinished task from earlier days onto `date` (oldest first), so
    /// nothing gets lost in the history. Finished tasks stay where they were.
    pub fn move_unfinished_to(&mut self, date: NaiveDate, now_ms: i64) -> Result<u32> {
        self.reload_if_changed()?;
        let earlier: Vec<NaiveDate> = {
            let mut days: Vec<_> = self.data.todos_by_date.keys().filter(|d| **d < date).copied().collect();
            days.sort();
            days
        };
        let mut moved = Vec::new();
        let mut changed = BTreeSet::from([Change::Todos(date)]);
        for day in earlier {
            if let Some(todos) = self.data.todos_by_date.get_mut(&day) {
                let (unfinished, finished): (Vec<_>, Vec<_>) = todos.drain(..).partition(|t| !t.completed);
                *todos = finished;
                if !unfinished.is_empty() {
                    changed.insert(Change::Todos(day));
                }
                moved.extend(unfinished);
            }
        }
        let count = moved.len() as u32;
        if count > 0 {
            self.data.todos_by_date.entry(date).or_default().extend(moved);
        }
        self.data.todos_by_date.retain(|_, todos| !todos.is_empty());
        self.save(now_ms, changed)?;
        Ok(count)
    }

    /// Days from `first` to `last` that have tasks: `true` if some are still unfinished.
    pub fn task_marks(&self, first: NaiveDate, last: NaiveDate) -> HashMap<NaiveDate, bool> {
        self.data
            .todos_by_date
            .iter()
            .filter(|(day, todos)| (first..=last).contains(*day) && !todos.is_empty())
            .map(|(day, todos)| (*day, todos.iter().any(|todo| !todo.completed)))
            .collect()
    }
}

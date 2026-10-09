use jiff::{Timestamp, ToSpan};
use kru::bucket::Bucket;
use kru::task::{Priority, Status, Task};

fn main() {
    // --- Building tasks -----------------------------------------------------
    // `new` sets defaults (Low priority, ToDo, no due date); `with_*` overrides them.
    let tomorrow = Timestamp::now() + 24.hours();
    let next_week = Timestamp::now() + (7 * 24).hours();

    let mut home = Bucket::new("Home");
    let groceries = home.add(
        Task::new("Buy groceries")
            .with_priority(Priority::High)
            .with_due(tomorrow),
    );
    home.add(Task::new("Clean the garage"));
    let plants = home.add(Task::new("Water the plants").with_priority(Priority::Medium));

    let mut work = Bucket::new("Work");
    let report = work.add(
        Task::new("Write quarterly report")
            .with_priority(Priority::High)
            .with_due(next_week),
    );
    work.add(Task::new("Review pull requests").with_priority(Priority::Medium));
    work.add(Task::new("Book flights for the offsite"));

    // --- Mutating tasks in place --------------------------------------------
    // `get_mut` borrows the task out of the bucket; `set_status` edits it in place.
    if let Some(task) = home.get_mut(groceries) {
        task.set_status(Status::InProgress);
    }
    work.get_mut(report).unwrap().set_status(Status::Done);

    let buckets = vec![home, work];

    println!("== All buckets ==");
    for bucket in &buckets {
        print_bucket(bucket);
    }

    // --- Querying -----------------------------------------------------------
    println!("\n== Search \"the\" across all buckets ==");
    for bucket in &buckets {
        for task in bucket.search("the") {
            println!("  [{}] {}", bucket.name, task.description);
        }
    }

    println!("\n== ToDo tasks in Home ==");
    for task in buckets[0].with_status(Status::ToDo) {
        print_task(task);
    }

    // Queries return iterators, so they compose with the usual adapters.
    println!("\n== High priority, not done (all buckets) ==");
    let urgent = buckets
        .iter()
        .flat_map(|b| b.iter())
        .filter(|t| t.priority == Priority::High && t.status != Status::Done);
    for task in urgent {
        print_task(task);
    }

    // --- Lookup and removal by id -------------------------------------------
    let mut home = buckets.into_iter().next().unwrap();
    println!("\n== Lookup by id ==");
    match home.get(plants) {
        Some(task) => print_task(task),
        None => println!("  not found"),
    }

    let removed = home.remove(plants).expect("task exists");
    println!("\n== Removed \"{}\" ==", removed.description);
    println!("  still there? {}", home.get(plants).is_some());
    print_bucket(&home);

    // --- Serialization ------------------------------------------------------
    println!("\n== Task as JSON ==");
    let task = home.get(groceries).unwrap();
    println!("{}", serde_json::to_string_pretty(task).unwrap());

    println!("\n== Bucket as JSON ==");
    println!("{}", serde_json::to_string_pretty(&home).unwrap());
}

fn print_bucket(bucket: &Bucket) {
    println!("\n{} ({} tasks)", bucket.name, bucket.iter().count());
    for task in bucket.iter() {
        print_task(task);
    }
}

fn print_task(task: &Task) {
    let due = match task.due {
        Some(due) => due.strftime("%Y-%m-%d %H:%M").to_string(),
        None => "-".to_string(),
    };
    println!(
        "  {:<30} {:<8} {:<11} due {}",
        task.description,
        format!("{:?}", task.priority),
        format!("{:?}", task.status),
        due,
    );
}

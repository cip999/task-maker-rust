use itertools::Itertools;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use task_maker_format::ioi::{
    CompilationStatus, SolutionEvaluationState, SubtaskId, Task, TestcaseEvaluationStatus, UIState,
};
use task_maker_format::ui::UIMessage;
use task_maker_format::EvaluationConfig;

/// Interface for testing a task.
#[derive(Debug)]
pub struct TestInterface {
    /// The path to the task directory.
    pub path: PathBuf,
    /// The time limit of the task.
    pub time_limit: Option<f64>,
    /// The memory limit of the task.
    pub memory_limit: Option<u64>,
    /// The maximum score of the task.
    pub max_score: Option<f64>,
    /// The list of the names of the files that must compile.
    pub must_compile: Vec<PathBuf>,
    /// The list of the names of the files that must fail to compile.
    pub must_not_compile: Vec<PathBuf>,
    /// The list of the names of the files that should not be compiled.
    pub not_compiled: Vec<PathBuf>,
    /// The list of the scores of the subtasks.
    pub subtask_scores: Option<Vec<f64>>,
    /// The list of scores, for each subtask, of the solutions.
    pub solution_scores: HashMap<PathBuf, Vec<f64>>,
    /// The status of the evaluation of some solutions.
    pub solution_statuses: HashMap<PathBuf, Vec<TestcaseEvaluationStatus>>,
}

impl TestInterface {
    /// Make a new `TestInterface` from the specified task directory.
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tasks")
            .join(path.into());
        TestInterface {
            path,
            time_limit: None,
            memory_limit: None,
            max_score: None,
            must_compile: Vec::new(),
            must_not_compile: Vec::new(),
            not_compiled: Vec::new(),
            subtask_scores: None,
            solution_scores: HashMap::new(),
            solution_statuses: HashMap::new(),
        }
    }

    /// Check that the time limit is the one specified.
    pub fn time_limit(&mut self, time_limit: f64) -> &mut Self {
        self.time_limit = Some(time_limit);
        self
    }

    /// Check that the memory limit is the one specified.
    pub fn memory_limit(&mut self, memory_limit: u64) -> &mut Self {
        self.memory_limit = Some(memory_limit);
        self
    }

    /// Check that the max score of the task is the one specified.
    pub fn max_score(&mut self, max_score: f64) -> &mut Self {
        self.max_score = Some(max_score);
        self
    }

    /// Check that the specified file is compiled successfully.
    pub fn must_compile<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.must_compile.push(source.into());
        self
    }

    /// Check that the specified file fails to compile.
    pub fn must_not_compile<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.must_not_compile.push(source.into());
        self
    }

    /// Check that the specified file is not compiled.
    pub fn not_compiled<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.not_compiled.push(source.into());
        self
    }

    /// Check that the subtasks have the following scores.
    pub fn subtask_scores<I: IntoIterator<Item = f64>>(&mut self, scores: I) -> &mut Self {
        self.subtask_scores = Some(scores.into_iter().collect());
        self
    }

    /// Check that the solution scores those values for each subtask.
    pub fn solution_score<P: Into<PathBuf>, I: IntoIterator<Item = f64>>(
        &mut self,
        solution: P,
        scores: I,
    ) -> &mut Self {
        self.solution_scores
            .entry(solution.into())
            .or_insert(scores.into_iter().collect());
        self
    }

    /// Check that the statuses of the solution starts with the ones specified.
    pub fn solution_statuses<P: Into<PathBuf>, I: IntoIterator<Item = TestcaseEvaluationStatus>>(
        &mut self,
        solution: P,
        statuses: I,
    ) -> &mut Self {
        self.solution_statuses
            .entry(solution.into())
            .or_insert(statuses.into_iter().collect());
        self
    }

    /// Spawn task-maker, reading its json output and checking that all the checks are good.
    pub fn run(&self) {
        println!("Expecting: {:#?}", self);
        let task = Task::new(
            &self.path,
            &EvaluationConfig {
                solution_filter: vec![],
                booklet_solutions: false,
            },
        )
        .unwrap();
        let task_maker = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target")
            .join("debug")
            .join("task-maker");
        let mut command = Command::new(task_maker);
        command.arg("--task-dir").arg(&self.path);
        command.arg("--ui").arg("json");
        command.arg("--no-cache");
        command.arg("--dry-run");
        command.env("RUST_BACKTRACE", "1");
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());
        let output = command.output().unwrap();
        if !output.status.success() {
            eprintln!("{:?}", String::from_utf8_lossy(&output.stderr));
            panic!("task-maker exited with: {:?}", output.status);
        }
        let mut state = UIState::new(&task);
        for message in String::from_utf8(output.stdout).unwrap().lines() {
            let message = serde_json::from_str::<UIMessage>(&message).expect("Invalid message");
            state.apply(message);
        }
        println!("State is: {:#?}", state);
        self.check_limits(&state);
        self.check_compilation(&state);
        self.check_subtasks(&state);
        self.check_solution_scores(&state);
        self.check_solution_statuses(&state);
    }

    /// Check the task limits are met.
    fn check_limits(&self, state: &UIState) {
        if let (Some(expected), Some(actual)) = (self.time_limit, state.task.time_limit) {
            assert!(abs_diff_eq!(expected, actual), "Wrong time limit");
        }
        if let (Some(expected), Some(actual)) = (self.memory_limit, state.task.memory_limit) {
            assert_eq!(expected, actual, "Wrong memory limit");
        }
        if let Some(max_score) = self.max_score {
            assert!(abs_diff_eq!(max_score, state.max_score), "Wrong max score");
        }
    }

    /// Check that the compilation of the files is good.
    fn check_compilation(&self, state: &UIState) {
        let compilations: HashMap<PathBuf, &CompilationStatus> = state
            .compilations
            .iter()
            .map(|(file, comp)| (PathBuf::from(file.file_name().unwrap()), comp))
            .collect();
        for name in self.must_compile.iter() {
            if compilations.contains_key(name) {
                match compilations[name] {
                    CompilationStatus::Done { .. } => {}
                    _ => panic!(
                        "Expecting {:?} to compile, but was {:?}",
                        name, compilations[name]
                    ),
                }
            } else {
                panic!("Expecting {:?} to compile, but was not in the UI", name);
            }
        }
        for name in self.must_not_compile.iter() {
            if compilations.contains_key(name) {
                match compilations[name] {
                    CompilationStatus::Failed { .. } => {}
                    _ => panic!(
                        "Expecting {:?} to not compile, but was {:?}",
                        name, compilations[name]
                    ),
                }
            } else {
                panic!("Expecting {:?} to not compile, but was not in the UI", name);
            }
        }
        for name in self.not_compiled.iter() {
            if compilations.contains_key(name) {
                panic!(
                    "Expecting {:?} not to be compiled, but was {:?}",
                    name, compilations[name]
                );
            }
        }
    }

    /// Check that the score of the subtasks are good.
    fn check_subtasks(&self, state: &UIState) {
        if let Some(scores) = &self.subtask_scores {
            assert_eq!(
                scores.len(),
                state.task.subtasks.len(),
                "Subtask len mismatch"
            );
            for i in 0..scores.len() {
                let expected = scores[i];
                let actual = state.task.subtasks[&(i as SubtaskId)].max_score;
                assert!(abs_diff_eq!(expected, actual), "Subtask score mismatch");
            }
        }
    }

    /// Check that the scores of the solutions are good.
    fn check_solution_scores(&self, state: &UIState) {
        let evaluations: HashMap<PathBuf, &SolutionEvaluationState> = state
            .evaluations
            .iter()
            .map(|(file, eval)| (PathBuf::from(file.file_name().unwrap()), eval))
            .collect();
        for (name, scores) in self.solution_scores.iter() {
            let state = evaluations[name];
            let score: f64 = scores.iter().sum();
            assert!(
                abs_diff_eq!(score, state.score.unwrap()),
                "Solution score mismatch: {} != {}",
                score,
                state.score.unwrap()
            );
            assert_eq!(
                scores.len(),
                state.subtasks.len(),
                "Wrong number of subtask"
            );
            for st in 0..scores.len() {
                let expected = scores[st];
                let actual = state.subtasks[&(st as SubtaskId)].score.unwrap();
                assert!(
                    abs_diff_eq!(expected, actual),
                    "Solution subtask score mismatch: {} != {}",
                    expected,
                    actual
                );
            }
        }
    }

    /// Check that the statuses of the solutions are good.
    fn check_solution_statuses(&self, state: &UIState) {
        let evaluations: HashMap<PathBuf, Vec<TestcaseEvaluationStatus>> = state
            .evaluations
            .iter()
            .map(|(file, eval)| {
                (
                    PathBuf::from(file.file_name().unwrap()),
                    eval.subtasks
                        .keys()
                        .sorted()
                        .flat_map(|st| {
                            eval.subtasks[st]
                                .testcases
                                .keys()
                                .sorted()
                                .map(move |tc| eval.subtasks[st].testcases[tc].status.clone())
                        })
                        .collect(),
                )
            })
            .collect();
        for (name, statuses) in self.solution_statuses.iter() {
            let actuals = &evaluations[name];
            for i in 0..actuals.len() {
                let actual = &actuals[i];
                let expected = if i < statuses.len() {
                    &statuses[i]
                } else {
                    &statuses[statuses.len() - 1]
                };
                assert_eq!(expected, actual, "Solution status mismatch of {:?}", name);
            }
        }
    }
}

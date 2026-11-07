use crate::Viewer;
use crate::lexer::LineState;
use crate::test_support::{named, pieces};
use crate::types::Format;
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Literal, Number, Punctuation, String as Str, TypeName,
};

fn sln(line: &str) -> Vec<(&str, TokenClass)> {
    pieces(line, LineState::Clear, Format::Solution).0
}

/// A solution as Visual Studio 2022 writes it: a byte-order mark, a blank first line, CRLF, tabs.
const SOLUTION: &str = "\u{feff}\r\nMicrosoft Visual Studio Solution File, Format Version 12.00\r\n# Visual Studio Version 17\r\nVisualStudioVersion = 17.0.31903.59\r\nMinimumVisualStudioVersion = 10.0.40219.1\r\nProject(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"App\", \"App\\App.csproj\", \"{5D3A1C2B-0000-4000-8000-000000000001}\"\r\n\tProjectSection(ProjectDependencies) = postProject\r\n\t\t{5D3A1C2B-0000-4000-8000-000000000002} = {5D3A1C2B-0000-4000-8000-000000000002}\r\n\tEndProjectSection\r\nEndProject\r\nGlobal\r\n\tGlobalSection(SolutionConfigurationPlatforms) = preSolution\r\n\t\tDebug|Any CPU = Debug|Any CPU\r\n\tEndGlobalSection\r\n\tGlobalSection(SolutionProperties) = preSolution\r\n\t\tHideSolutionNode = FALSE\r\n\tEndGlobalSection\r\nEndGlobal\r\n";

#[test]
fn a_solution_is_read_by_its_extension() {
    assert_eq!(Format::from_name("App.sln"), Format::Solution);
    assert_eq!(Format::from_name("APP.SLN"), Format::Solution);
}

#[test]
fn the_lines_of_a_solution() {
    assert_eq!(
        sln("Microsoft Visual Studio Solution File, Format Version 12.00"),
        [
            (
                "Microsoft Visual Studio Solution File, Format Version",
                Keyword
            ),
            ("12.00", Number)
        ]
    );
    assert_eq!(
        sln("# Visual Studio Version 17"),
        [("# Visual Studio Version 17", Comment)]
    );
    assert_eq!(
        sln("VisualStudioVersion = 17.0.31903.59"),
        [
            ("VisualStudioVersion", Key),
            ("=", Punctuation),
            ("17.0.31903.59", Number)
        ]
    );
    assert_eq!(
        sln(
            "Project(\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\") = \"App\", \"App\\App.csproj\", \"{5D3A1C2B-0000-4000-8000-000000000001}\""
        ),
        [
            ("Project", Keyword),
            ("(", Punctuation),
            ("\"{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}\"", Literal),
            (")", Punctuation),
            ("=", Punctuation),
            ("\"App\"", TypeName),
            (",", Punctuation),
            ("\"App\\App.csproj\"", Str),
            (",", Punctuation),
            ("\"{5D3A1C2B-0000-4000-8000-000000000001}\"", Literal)
        ]
    );
    assert_eq!(
        sln("\tGlobalSection(SolutionConfigurationPlatforms) = preSolution"),
        [
            ("GlobalSection", Keyword),
            ("(", Punctuation),
            ("SolutionConfigurationPlatforms", TypeName),
            (")", Punctuation),
            ("=", Punctuation),
            ("preSolution", Literal)
        ]
    );
    assert_eq!(
        sln("\t\tDebug|Any CPU.ActiveCfg = Debug|Any CPU"),
        [
            ("Debug|Any CPU.ActiveCfg", Key),
            ("=", Punctuation),
            ("Debug|Any CPU", Str)
        ]
    );
    assert_eq!(
        sln("\t\tHideSolutionNode = FALSE"),
        [
            ("HideSolutionNode", Key),
            ("=", Punctuation),
            ("FALSE", Literal)
        ]
    );
    for end in [
        "EndProject",
        "\tEndProjectSection",
        "Global",
        "\tEndGlobalSection",
        "EndGlobal",
    ] {
        assert_eq!(sln(end), [(end.trim_start(), Keyword)], "{end}");
    }
    assert_eq!(sln("nonsense"), [("nonsense", Invalid)]);
    assert_eq!(
        sln("Project(broken"),
        [("Project", Keyword), ("(broken", Invalid)]
    );
    assert!(sln("").is_empty());
}

/// The tree: the properties before the first project, then a node per project and section,
/// labelled with its name, holding its `key = value` lines.
#[test]
fn a_solution_unfolds_into_its_projects_and_sections() {
    let (mut viewer, _) =
        Viewer::open(named(SOLUTION.as_bytes(), "App.sln"), None).expect("the solution opens");
    assert_eq!(viewer.format(), Format::Solution);
    let top = viewer.children(None).expect("top");
    let labels: Vec<&str> = top.iter().map(|node| node.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "VisualStudioVersion",
            "MinimumVisualStudioVersion",
            "App",
            "ProjectDependencies",
            "SolutionConfigurationPlatforms",
            "SolutionProperties"
        ]
    );
    let platforms = viewer.children(Some(top[4].id)).expect("a section");
    let shown: Vec<(&str, Option<&str>)> = platforms
        .iter()
        .map(|node| (node.label.as_str(), node.value.as_deref()))
        .collect();
    assert_eq!(shown, [("Debug|Any CPU", Some("Debug|Any CPU"))]);
}

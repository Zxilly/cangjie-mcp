"""Tests for indexer/api_extractor.py."""

from cangjie_mcp.indexer.api_extractor import (
    extract_method_signatures,
    extract_stdlib_info,
    extract_type_declarations,
)


class TestExtractStdlibInfo:
    """Tests for extract_stdlib_info function."""

    def test_detects_stdlib_imports(self) -> None:
        """Test that stdlib imports are detected."""
        content = """
# ArrayList Usage

```cangjie
import std.collection.*

let list = ArrayList<Int64>()
```
"""
        result = extract_stdlib_info(content)
        assert result["is_stdlib"] is True
        assert "std.collection" in result["packages"]

    def test_multiple_packages(self) -> None:
        """Test detection of multiple packages."""
        content = """
```cangjie
import std.collection.ArrayList
import std.fs.File
import std.net.Socket
```
"""
        result = extract_stdlib_info(content)
        assert result["is_stdlib"] is True
        assert len(result["packages"]) == 3
        assert "std.collection.ArrayList" in result["packages"]
        assert "std.fs.File" in result["packages"]
        assert "std.net.Socket" in result["packages"]

    def test_no_stdlib_imports(self) -> None:
        """Test content without stdlib imports."""
        content = """
# Variables

```cangjie
let x = 42
var y = "hello"
```
"""
        result = extract_stdlib_info(content)
        assert result["is_stdlib"] is False
        assert result["packages"] == []

    def test_detects_import_std_in_text(self) -> None:
        """Test that 'import std.' in text is detected."""
        content = """
You can use import std.collection to access collections.
"""
        result = extract_stdlib_info(content)
        assert result["is_stdlib"] is True


class TestExtractTypeDeclarations:
    """Tests for extract_type_declarations function."""

    def test_extract_class(self) -> None:
        """Test extracting class declarations."""
        content = """
```cangjie
public class MyContainer<T> {
    var items: ArrayList<T>
}
```
"""
        types = extract_type_declarations(content)
        assert "MyContainer" in types

    def test_extract_struct(self) -> None:
        """Test extracting struct declarations."""
        content = """
```cangjie
struct Point {
    var x: Int64
    var y: Int64
}
```
"""
        types = extract_type_declarations(content)
        assert "Point" in types

    def test_extract_interface(self) -> None:
        """Test extracting interface declarations."""
        content = """
```cangjie
interface Drawable {
    func draw(): Unit
}
```
"""
        types = extract_type_declarations(content)
        assert "Drawable" in types

    def test_extract_enum(self) -> None:
        """Test extracting enum declarations."""
        content = """
```cangjie
enum Color {
    | Red | Green | Blue
}
```
"""
        types = extract_type_declarations(content)
        assert "Color" in types

    def test_extract_multiple_types(self) -> None:
        """Test extracting multiple type declarations."""
        content = """
```cangjie
public open class Animal { }
class Dog <: Animal { }
struct Point { var x: Int64 }
interface Runnable { func run(): Unit }
enum Status { | Active | Inactive }
```
"""
        types = extract_type_declarations(content)
        assert "Animal" in types
        assert "Dog" in types
        assert "Point" in types
        assert "Runnable" in types
        assert "Status" in types


class TestExtractMethodSignatures:
    """Tests for extract_method_signatures function."""

    def test_extract_simple_func(self) -> None:
        """Test extracting simple function."""
        content = """
```cangjie
func add(a: Int64, b: Int64): Int64 {
    a + b
}
```
"""
        methods = extract_method_signatures(content)
        assert len(methods) == 1
        assert methods[0]["name"] == "add"
        assert methods[0]["return_type"] == "Int64"

    def test_extract_public_func(self) -> None:
        """Test extracting public function."""
        content = """
```cangjie
public func greet(name: String): Unit {
    println("Hello ${name}")
}
```
"""
        methods = extract_method_signatures(content)
        assert len(methods) == 1
        assert methods[0]["name"] == "greet"

    def test_extract_static_func(self) -> None:
        """Test extracting static function."""
        content = """
```cangjie
public static func create(): MyClass {
    MyClass()
}
```
"""
        methods = extract_method_signatures(content)
        assert len(methods) == 1
        assert methods[0]["name"] == "create"

    def test_extract_generic_func(self) -> None:
        """Test extracting generic function."""
        content = """
```cangjie
func identity<T>(value: T): T {
    value
}
```
"""
        methods = extract_method_signatures(content)
        assert len(methods) == 1
        assert methods[0]["name"] == "identity"

    def test_no_duplicates(self) -> None:
        """Test that duplicate method names are not returned."""
        content = """
```cangjie
func process(): Unit { }
func process(): Unit { }
func process(): Unit { }
```
"""
        methods = extract_method_signatures(content)
        assert len(methods) == 1
        assert methods[0]["name"] == "process"

    def test_multiple_methods(self) -> None:
        """Test extracting multiple methods."""
        content = """
```cangjie
public func add(a: Int64, b: Int64): Int64 { a + b }
public func subtract(a: Int64, b: Int64): Int64 { a - b }
public func multiply(a: Int64, b: Int64): Int64 { a * b }
```
"""
        methods = extract_method_signatures(content)
        names = [m["name"] for m in methods]
        assert "add" in names
        assert "subtract" in names
        assert "multiply" in names

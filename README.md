Documentation: [![Documentation](https://docs.rs/graphannis/badge.svg)](https://docs.rs/graphannis/) / Build status: [![Build Status Linux & MacOS X](https://travis-ci.org/korpling/graphANNIS.svg?branch=develop)](https://travis-ci.org/korpling/graphANNIS) (Linux & MacOS)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/27axqoanq6rj3xps/branch/develop?svg=true)](https://ci.appveyor.com/project/thomaskrause/graphannis/branch/develop) (Windows)

graphANNIS
==========

This is a new backend implementation of the ANNIS linguistic search and visualization system (http://corpus-tools.org/annis/). 

While there are test cases, this project is still in a pre-release beta phase. 
**Only a sub-set of the ANNIS Query Langugage (AQL) is supported yet (full support is planned).**
Integration with ANNIS is currently implemented in a special branch: https://github.com/korpling/ANNIS/tree/feature/graphannis


How to compile
---------------

graphANNIS is written in the Rust programming language (https://www.rust-lang.org).
You can install Rust from https://www.rust-lang.org/en-US/install.html.
After you have installed Rust, you can can build the complete project with

```
cargo build --release --all-features
```

3rd party dependencies
----------------------

This software depends on several 3rd party libraries. These are documented in the BOM.txt file in this folder.

Language bindings
------------------

- Java: https://github.com/korpling/graphANNIS-java
- Python 3: https://github.com/korpling/graphANNIS-python

Author(s)
---------

* Thomas Krause (thomaskrause@posteo.de)

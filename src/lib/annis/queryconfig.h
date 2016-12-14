#pragma once

#include <map>

#include <annis/types.h>

namespace annis
{
  struct QueryConfig
  {
    bool optimize;
    bool forceFallback;
    std::map<Component, std::string> overrideImpl;
    size_t numOfParallelTasks;
  public:
    QueryConfig();
  };
}


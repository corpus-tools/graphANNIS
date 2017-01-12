/*
   Copyright 2017 Thomas Krause <thomaskrause@posteo.de>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
*/

#include "taskindexjoin.h"

#include <annis/operators/operator.h>     // for Operator
#include <annis/util/comparefunctions.h>  // for checkAnnotationEqual
#include <algorithm>                      // for move
#include <future>                         // for future, async, launch, laun...
#include <list>                           // for list
#include "annis/iterators.h"              // for AnnoIt, Iterator
#include "annis/types.h"                  // for Match, Annotation, nodeid_t
#include "annis/util/threadpool.h"        // for ThreadPool



using namespace annis;

TaskIndexJoin::TaskIndexJoin(std::shared_ptr<Iterator> lhs, size_t lhsIdx,
                     std::shared_ptr<Operator> op,
                     std::function<std::list<Annotation>(nodeid_t)> matchGeneratorFunc, unsigned maxBufferedTasks,
                     std::shared_ptr<ThreadPool> threadPool)
  : lhs(lhs), lhsIdx(lhsIdx), maxNumfOfTasks(maxBufferedTasks > 0 ? maxBufferedTasks : 1), workerPool(threadPool),
    taskBufferSize(0)
{


  taskBufferGenerator = [matchGeneratorFunc, op, lhsIdx](const std::vector<Match>& currentLHS) -> std::list<MatchPair>
  {
    std::list<MatchPair> result;

    std::unique_ptr<AnnoIt> reachableNodesIt = op->retrieveMatches(currentLHS[lhsIdx]);
    if(reachableNodesIt)
    {
      Match reachableNode;
      while(reachableNodesIt->next(reachableNode))
      {
        for(Annotation currentRHSAnno : matchGeneratorFunc(reachableNode.node))
        {
          if((op->isReflexive() || currentLHS[lhsIdx].node != reachableNode.node
          || !checkAnnotationEqual(currentLHS[lhsIdx].anno, currentRHSAnno)))
          {
            result.push_back({currentLHS, {reachableNode.node, currentRHSAnno}});
          }
        }
      }
    }

    return std::move(result);
  };
}

bool TaskIndexJoin::next(std::vector<Match> &tuple)
{
  tuple.clear();

  do
  {
    while(!matchBuffer.empty())
    {
      const MatchPair& m = matchBuffer.front();

      tuple.reserve(m.lhs.size()+1);
      tuple.insert(tuple.begin(), m.lhs.begin(), m.lhs.end());
      tuple.push_back(m.rhs);

      matchBuffer.pop_front();
      return true;

    }
  } while (nextMatchBuffer());

  return false;
}

void TaskIndexJoin::reset()
{
  if(lhs)
  {
    lhs->reset();
  }

  matchBuffer.clear();
  taskBuffer.clear();
  taskBufferSize = 0;
}


bool TaskIndexJoin::fillTaskBuffer()
{
  std::vector<Match> currentLHS;
  while(taskBufferSize < maxNumfOfTasks && lhs->next(currentLHS))
  {
    if(workerPool)
    {
      taskBuffer.push_back(workerPool->enqueue(taskBufferGenerator, currentLHS));
    }
    else
    {
      // do not use threads
      taskBuffer.push_back(std::async(std::launch::deferred, taskBufferGenerator, currentLHS));
    }
    taskBufferSize++;
  }

  return !taskBuffer.empty();
}

bool TaskIndexJoin::nextMatchBuffer()
{
  while(fillTaskBuffer())
  {
    matchBuffer = std::move(taskBuffer.front().get());
    taskBuffer.pop_front();
    taskBufferSize--;

    // if there is a non empty result return true, otherwise try more entries of the task buffer
    if(!matchBuffer.empty())
    {
      return true;
    }
  }

  // return false if there is no more value to fetch from the task buffer.
  return false;
}

TaskIndexJoin::~TaskIndexJoin()
{
}

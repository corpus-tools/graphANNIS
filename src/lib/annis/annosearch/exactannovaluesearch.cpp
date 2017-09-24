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

#include "exactannovaluesearch.h"

#include <google/btree.h>                // for btree_iterator
#include <google/btree_map.h>            // for btree_map
#include <boost/container/flat_map.hpp>  // for flat_multimap
#include <boost/container/vector.hpp>    // for vec_iterator, operator!=
#include <cstdint>                       // for uint32_t, int64_t
#include "annis/db.h"                    // for DB
#include "annis/stringstorage.h"         // for StringStorage
#include "annis/annostorage.h"           // for AnnoStorage
#include "annis/types.h"                 // for Annotation, AnnotationKey


using namespace annis;
using namespace std;

ExactAnnoValueSearch::ExactAnnoValueSearch(const DB &db, const string &annoNamspace, const string &annoName, const string &annoValue)
  :db(db),validAnnotationInitialized(false), debugDescription(annoNamspace + ":" + annoName + "=\"" + annoValue + "\"")
{
  auto nameID = db.strings.findID(annoName);
  auto namspaceID = db.strings.findID(annoNamspace);
  auto valueID = db.strings.findID(annoValue);

  if(nameID && namspaceID && valueID)
  {
    Annotation key;
    key.name = *nameID;
    key.ns = *namspaceID;
    key.val = *valueID;

    searchRanges.push_back(Range(db.nodeAnnos.inverseAnnotations.equal_range(key)));
    it = searchRanges.begin()->first;
  }
  currentRange = searchRanges.begin();
}

ExactAnnoValueSearch::ExactAnnoValueSearch(const DB &db, const std::string &annoName, const std::string &annoValue)
  :db(db), validAnnotationInitialized(false), debugDescription(annoName + "=\"" + annoValue + "\"")
{
  auto nameID = db.strings.findID(annoName);
  auto valueID = db.strings.findID(annoValue);

  if(nameID && valueID)
  {
    auto keysLower = db.nodeAnnos.annoKeys.lower_bound({*nameID, 0});
    auto keysUpper = db.nodeAnnos.annoKeys.upper_bound({*nameID, uintmax});
    for(auto itKey = keysLower; itKey != keysUpper; itKey++)
    {
      Range r = db.nodeAnnos.inverseAnnotations.equal_range(
        {itKey->first.name, itKey->first.ns, *valueID});

      // only remember ranges that actually have valid iterator pairs
      if(r.first != r.second)
      {
        searchRanges.push_back(std::move(r));
      }
    }
  }
  currentRange = searchRanges.begin();

  if(currentRange != searchRanges.end())
  {
    it = currentRange->first;
  }
}

bool ExactAnnoValueSearch::next(Match& result)
{
  while(currentRange != searchRanges.end() && it != currentRange->second)
  {
    result.node = it->second; // node ID
    result.anno = it->first; // annotation itself

    it++;
    if(it == currentRange->second)
    {
      currentRange++;
      if(currentRange != searchRanges.end())
      {
        it = currentRange->first;
      }
    }

    if(getConstAnnoValue())
    {
      /*
       * When we replace the resulting annotation with a constant value it is possible that duplicates
       * can occur. Therfore we must check that each node is only included once as a result
       */
      if(uniqueResultFilter.find(result.node) == uniqueResultFilter.end())
      {
        uniqueResultFilter.insert(result.node);

        result.anno = *getConstAnnoValue();

        return true;
      }
    }
    else
    {
      return true;
    }
  }

  return false;

}

void ExactAnnoValueSearch::reset()
{
  uniqueResultFilter.clear();

  currentRange = searchRanges.begin();
  if(currentRange != searchRanges.end())
  {
    it = currentRange->first;
  }
}

void ExactAnnoValueSearch::initializeValidAnnotations()
{
  for(auto range : searchRanges)
  {
    for(ItType annoIt = range.first; annoIt != range.second; annoIt++)
    {
      validAnnotations.insert(annoIt->first);
    }
  }

  validAnnotationInitialized = true;
}

std::int64_t ExactAnnoValueSearch::guessMaxCount() const
{
  std::int64_t sum = 0;

  for(auto range : searchRanges)
  {
    if(range.first != range.second)
    {
      const Annotation& anno = range.first->first;

      if(anno.ns == db.getNamespaceStringID() && anno.name == db.getNodeNameStringID())
      {
        // we know that node names are typically unique
        sum += 1;
      }
      else
      {
        const std::string val = db.strings.str(anno.val);
        sum += db.nodeAnnos.guessMaxCount(anno.ns, anno.name, val, val);
      }
    }
  }
  
  return sum;
}

ExactAnnoValueSearch::~ExactAnnoValueSearch()
{

}



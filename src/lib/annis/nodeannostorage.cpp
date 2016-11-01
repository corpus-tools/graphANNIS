/* 
 * File:   nodeannostorage.cpp
 * Author: thomas
 * 
 * Created on 14. Januar 2016, 13:53
 */

#include <annis/nodeannostorage.h>

#include <annis/stringstorage.h>

#include <re2/re2.h>

#include <cmath>
#include <fstream>
#include <random>

#include <cereal/archives/binary.hpp>
#include <cereal/types/map.hpp>
#include <cereal/types/set.hpp>

#include "annis/annosearch/annotationsearch.h"

#include <annis/util/size_estimator.h>

using namespace annis;


NodeAnnoStorage::NodeAnnoStorage(StringStorage& strings)
: strings(strings)
{
}

void NodeAnnoStorage::addNodeAnnotationBulk(std::list<std::pair<NodeAnnotationKey, uint32_t> > annos)
{

  annos.sort();
  nodeAnnotations.insert(annos.begin(), annos.end());

  std::list<std::pair<Annotation, nodeid_t>> inverseAnnos;
  std::list<AnnotationKey> annoKeyList;

  for(const auto& entry : annos)
  {
    const NodeAnnotationKey& key = entry.first;
    inverseAnnos.push_back(std::pair<Annotation, nodeid_t>({key.anno_name, key.anno_ns, entry.second}, key.node));
    annoKeyList.push_back({key.anno_name, key.anno_ns});
  }

  inverseAnnos.sort();

  inverseNodeAnnotations.insert(inverseAnnos.begin(), inverseAnnos.end());

  nodeAnnoKeys.insert(annoKeyList.begin(), annoKeyList.end());

}

void NodeAnnoStorage::clear()
{
  nodeAnnotations.clear();
  inverseNodeAnnotations.clear();
  
  histogramBounds.clear();
  nodeAnnotationKeyCount.clear();
}

size_t NodeAnnoStorage::estimateMemorySize()
{
  return
      size_estimation::element_size(nodeAnnotations)
      + size_estimation::element_size(inverseNodeAnnotations)
      + size_estimation::element_size(nodeAnnoKeys)
      + size_estimation::element_size(histogramBounds)
      + size_estimation::element_size(nodeAnnotationKeyCount);
}

bool NodeAnnoStorage::hasStatistics() const
{
  return !histogramBounds.empty() && !nodeAnnotationKeyCount.empty();
}


void NodeAnnoStorage::calculateStatistics()
{
  
  const size_t maxHistogramBuckets = 250;
  const size_t maxSampledAnnotations = 2500;
  
  histogramBounds.clear();
  nodeAnnotationKeyCount.clear();
  
  // collect statistics for each annotation key separatly
  std::map<AnnotationKey, std::vector<std::string>> globalValueList;
  for(const auto& annoKey : nodeAnnoKeys)
  {
    histogramBounds[annoKey] = std::vector<std::string>();
    auto& valueList = globalValueList[annoKey] = std::vector<std::string>();
    
    // get all annotations
    Annotation minAnno = {annoKey.name, annoKey.ns, 0};
    Annotation maxAnno = {annoKey.name, annoKey.ns, std::numeric_limits<std::uint32_t>::max()};
    auto itUpperBound = inverseNodeAnnotations.upper_bound(maxAnno);
    std::vector<Annotation> annos;
    for(auto it=inverseNodeAnnotations.lower_bound(minAnno); it != itUpperBound; it++)
    {
      annos.push_back(it->first);
      auto itKeyCount = nodeAnnotationKeyCount.find(annoKey);
      if(itKeyCount == nodeAnnotationKeyCount.end())
      {
        nodeAnnotationKeyCount[annoKey] = 1;
      }
      else
      {
        auto newVal = itKeyCount->second+1;
        nodeAnnotationKeyCount[annoKey] = newVal;
      }
    }
    std::random_shuffle(annos.begin(), annos.end());
    valueList.resize(std::min<size_t>(maxSampledAnnotations, annos.size()));
    for(size_t i=0; i < valueList.size(); i++)
    {
      valueList[i] = strings.str(annos[i].val);
    }
    
  }
  
  // create uniformly distributed histogram bounds for each node annotation key 
  for(auto it=globalValueList.begin(); it != globalValueList.end(); it++)
  {
    auto& values = it->second;
    
    std::sort(values.begin(), values.end());
    
    size_t numValues = values.size();
    
    size_t numHistBounds = maxHistogramBuckets + 1;
    if(numValues < numHistBounds)
    {
      numHistBounds = numValues;
    }
    
    if(numHistBounds >= 2)
    {
      auto& h = histogramBounds[it->first];
      h.resize(numHistBounds);

      std::int64_t delta = (numValues-1) / (numHistBounds -1);
      std::int64_t deltaFraction = (numValues -1) % (numHistBounds - 1);

	  std::int64_t pos = 0;
	  size_t posFraction = 0;
      for(size_t i=0; i < numHistBounds; i++)
      {
        h[i] = values[pos];
        pos += delta;
        posFraction += deltaFraction;
        
        if(posFraction >= (numHistBounds - 1))
        {
          pos++;
          posFraction -= (numHistBounds - 1);
        }
      }
    }
  }
}


std::int64_t NodeAnnoStorage::guessMaxCount(const std::string& ns, const std::string& name, const std::string& val) const
{
  auto nameID = strings.findID(name);
  if(nameID.first)
  {
    auto nsID = strings.findID(ns);
    if(nsID.first)
    {
      return guessMaxCount(boost::optional<std::uint32_t>(nsID.second), nameID.second, 
        val, val);
    }
  }
  
  
  // if none of the conditions above is valid the annotation key does not exist
  return 0;
}

std::int64_t NodeAnnoStorage::guessMaxCount(const std::string& name, const std::string& val) const
{
  auto nameID = strings.findID(name);
  if(nameID.first)
  {
    return guessMaxCount(boost::optional<std::uint32_t>(), nameID.second, val, val);
  }
  return 0;
}

std::int64_t NodeAnnoStorage::guessMaxCountRegex(const std::string& ns, const std::string& name, const std::string& val) const
{
  auto nameID = strings.findID(name);
  if(nameID.first)
  {
    auto nsID = strings.findID(ns);
    if(nsID.first)
    {
      re2::RE2 pattern(val);
      if(pattern.ok())
      {
        std::string minMatch;
        std::string maxMatch;
        pattern.PossibleMatchRange(&minMatch, &maxMatch, 10);
        return guessMaxCount(boost::optional<std::uint32_t>(nsID.second), nameID.second, minMatch, maxMatch);
      }
    }
  }
  
  return 0;
}

std::int64_t NodeAnnoStorage::guessMaxCountRegex(const std::string& name, const std::string& val) const
{
  auto nameID = strings.findID(name);
  if(nameID.first)
  {
    re2::RE2 pattern(val);
    if(pattern.ok())
    {
      std::string minMatch;
      std::string maxMatch;
      pattern.PossibleMatchRange(&minMatch, &maxMatch, 10);
      return guessMaxCount(boost::optional<std::uint32_t>(), nameID.second, minMatch, maxMatch);
    }
  }
  return 0;
}


std::int64_t NodeAnnoStorage::guessMaxCount(boost::optional<std::uint32_t> nsID, 
  std::uint32_t nameID, 
  const std::string& lowerVal, const std::string& upperVal) const
{
  std::list<AnnotationKey> keys;
  if(nsID)
  {
    keys.push_back({nameID, *nsID});
  }
  else
  {
    // find all complete keys which have the given name
    auto itKeyUpper = nodeAnnoKeys.upper_bound({nameID, std::numeric_limits<std::uint32_t>::max()});
    for(auto itKeys = nodeAnnoKeys.lower_bound({nameID, 0}); itKeys != itKeyUpper; itKeys++)
    {
      keys.push_back(*itKeys);
    }
  }
  
  std::int64_t universeSize = 0;
  std::int64_t sumHistogramBuckets = 0;
  std::int64_t countMatches = 0;
  // guess for each annotation fully qualified key and return the sum of all guesses
  for(const auto& key : keys)
  {
    auto itKeyCount = nodeAnnotationKeyCount.find(key);
    if(itKeyCount != nodeAnnotationKeyCount.end())
    {
      universeSize += itKeyCount->second;
    }
    auto itHisto = histogramBounds.find(key);
    if(itHisto != histogramBounds.end())
    {
      // find the range in which the value is contained
      const auto& histo = itHisto->second;
      
      // we need to make sure the histogram is not empty -> should have at least two bounds
      if(histo.size() >= 2)
      {
        sumHistogramBuckets += (histo.size() - 1);
        
        for(size_t i = 0; i < (histo.size()-1); i++)
        {
          const auto& bucketBegin = histo[i];
          const auto& bucketEnd = histo[i+1];
          // check if the range overlaps with the search range
          if(bucketBegin <= upperVal && lowerVal <= bucketEnd)
          {
            countMatches++;
          }
        }
      }
    }
  }
  
  if(sumHistogramBuckets > 0)
  {
    double selectivity = ((double) countMatches) / ((double) sumHistogramBuckets);
    return std::round(selectivity * ((double) universeSize));
  }
  else
  {
    return 0;
  }
  
}



NodeAnnoStorage::~NodeAnnoStorage()
{
}


#include <annis/stringstorage.h>
#include <fstream>
#include <annis/util/helper.h>

#include <re2/re2.h>

#include <annis/util/size_estimator.h>

using namespace annis;
using namespace std;

StringStorage::StringStorage()
{
}

std::set<std::uint32_t> StringStorage::findRegex(const string &str) const
{
  using ItType = btree::btree_map<string, uint32_t>::const_iterator;
  std::set<std::uint32_t> result;

  RE2 re(str, RE2::Quiet);
  if(re.ok())
  {
    // get the size of the last element so we know how large our prefix needs to be
    size_t prefixSize = 10;
    const std::string& lastString = stringStorageByValue.rbegin()->first;
    size_t lastStringSize = lastString.size()+1;
    if(lastStringSize > prefixSize)
    {
      prefixSize = lastStringSize;
    }

    std::string minPrefix;
    std::string maxPrefix;
    re.PossibleMatchRange(&minPrefix, &maxPrefix, prefixSize);

    ItType upperBound = stringStorageByValue.upper_bound(maxPrefix);

    for(ItType it=stringStorageByValue.lower_bound(minPrefix);
        it != upperBound; it++)
    {
      if(RE2::FullMatch(it->first, re))
      {
        result.insert(it->second);
      }
    }
  }

  return result;
}

uint32_t StringStorage::add(const string &str)
{
  typedef btree::btree_map<string, uint32_t>::const_iterator ItType;
  ItType it = stringStorageByValue.find(str);
  if(it == stringStorageByValue.end())
  {
    // non-existing
    uint32_t id = stringStorageByID.size() + 1; // since 0 is taken as ANY value begin with 1
    // make sure the ID is really not taken yet
    while(stringStorageByID.find(id) != stringStorageByID.end())
    {
      id++;
    }
    stringStorageByID.insert(pair<uint32_t, string>(id, str));
    stringStorageByValue.insert(pair<string, uint32_t>(str, id));
    return id;
  }
  else
  {
    // already existing, return the original ID
    return it->second;
  }
}

void StringStorage::clear()
{

  stringStorageByID.clear();
  stringStorageByValue.clear();

}


double annis::StringStorage::avgLength()
{
  size_t sum=0;
  for(const auto& v : stringStorageByValue)
  {
    sum += v.first.size();
  }
  return (double) sum / (double) stringStorageByValue.size();
}

size_t StringStorage::estimateMemorySize()
{
  return size_estimation::element_size(stringStorageByID) + size_estimation::element_size(stringStorageByValue);
}

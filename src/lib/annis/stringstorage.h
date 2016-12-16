#pragma once

#include <string>
#include <unordered_map>
#include <map>
#include <set>

#include <boost/optional.hpp>

#include <cereal/types/string.hpp>
#include <cereal/types/unordered_map.hpp>
#include <annis/serializers.h>


#include <google/btree_map.h>

namespace annis
{
const std::uint32_t STRING_STORAGE_ANY = 0;

class StringStorage
{
public:
  StringStorage();

  const std::string& str(std::uint32_t id) const
  {
    auto it = stringStorageByID.find(id);
    if(it != stringStorageByID.end())
    {
      return it->second;
    }
    else
    {
      throw("Unknown string ID");
    }
  }

  boost::optional<std::string> strOpt(std::uint32_t id) const
  {
    auto it = stringStorageByID.find(id);
    if(it != stringStorageByID.end())
    {
      return boost::optional<std::string>(it->second);
    }
    else
    {
      boost::optional<std::string>();
    }
  }

  std::pair<bool, std::uint32_t> findID(const std::string& str) const
  {
    typedef btree::btree_map<std::string, std::uint32_t>::const_iterator ItType;
    std::pair<bool, std::uint32_t> result;
    result.first = false;
    result.second = 0;
    ItType it = stringStorageByValue.find(str);
    if(it != stringStorageByValue.end())
    {
      result.first = true;
      result.second = it->second;
    }
    return result;
  }

  std::set<std::uint32_t> findRegex(const std::string& str) const;

  std::uint32_t add(const std::string& str);

  void clear();

  size_t size() {return stringStorageByID.size();}
  double avgLength();

  size_t estimateMemorySize();

  template<class Archive>
  void serialize(Archive & archive)
  {
    archive(stringStorageByID, stringStorageByValue);
  }

private:
  std::unordered_map<std::uint32_t, std::string> stringStorageByID;
  btree::btree_map<std::string, std::uint32_t> stringStorageByValue;

};
}


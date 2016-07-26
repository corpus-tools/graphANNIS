#pragma once

#include <map>
#include <unordered_map>

#include <boost/container/flat_map.hpp>
#include <boost/container/flat_set.hpp>

#include <google/btree_map.h>
#include <google/btree_set.h>

namespace annis
{
/**
 * Includes functions to estimate the used main memory of some containers in bytes.
 */
namespace size_estimation
{
/**
 * Estimate the memory usage of a map in bytes.
 */
template<typename Key, typename Value>
size_t memory(const std::map<Key, Value>& m)
{
  return (sizeof(typename std::map<Key, Value>::value_type) + sizeof(std::_Rb_tree_node_base)) * m.size() + sizeof(m);
}

template<typename Key, typename Value>
size_t memory(const std::unordered_map<Key, Value>& m)
{
  return (m.size() * sizeof(typename std::unordered_map<Key, Value>::value_type)) // actual elements stored
      + (m.bucket_count() * (sizeof(size_t) + sizeof(void*))) // head pointer per bucket
      + (m.size() * sizeof(void*)) // pointer for list entry of each element
      + sizeof(m);
}

template<typename Key>
size_t memory(const boost::container::flat_set<Key>& m)
{
  return (m.size() * sizeof(typename  boost::container::flat_set<Key>::value_type)) // actual elements stored
      + sizeof(m);
}

template<typename Key, typename Value>
size_t memory(const boost::container::flat_map<Key, Value>& m)
{
  return (m.size() * sizeof(typename boost::container::flat_map<Key, Value>::value_type)) // actual elements stored
      + sizeof(m);
}

template<typename Key, typename Value>
size_t memory(const boost::container::flat_multimap<Key, Value>& m)
{
  return (m.size() * sizeof(typename boost::container::flat_multimap<Key, Value>::value_type)) // actual elements stored
      + sizeof(m);
}

template<typename Key, typename Value>
size_t memory(const btree::btree_map<Key, Value>& m)
{
  return m.bytes_used();
}

template<typename Key, typename Value>
size_t memory(const btree::btree_multimap<Key, Value>& m)
{
  return m.bytes_used();
}

template<typename Key>
size_t memory(const  btree::btree_set<Key>& m)
{
  return m.bytes_used();
}

}
} // end namespace annis

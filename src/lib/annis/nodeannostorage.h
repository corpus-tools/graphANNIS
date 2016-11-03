/* 
 * File:   nodeannostorage.h
 * Author: thomas
 *
 * Created on 14. Januar 2016, 13:53
 */

#pragma once

#include <map>
#include <set>
#include <list>
#include <memory>

#include <google/btree_map.h>
#include <google/btree_set.h>

#include <boost/optional.hpp>
#include <boost/container/flat_map.hpp>
#include <boost/container/flat_set.hpp>
#include <boost/container/map.hpp>
#include <boost/container/set.hpp>

#include <cereal/cereal.hpp>
#include <cereal/types/map.hpp>
#include <cereal/types/set.hpp>
#include <cereal/types/vector.hpp>


#include <annis/types.h>
#include <annis/stringstorage.h>
#include <annis/serializers.h>

#include "iterators.h"

namespace annis {

  namespace bc = boost::container;


  class NodeAnnoStorage
  {
    friend class DB;
    friend class ExactAnnoValueSearch;
    friend class ExactAnnoKeySearch;
    friend class RegexAnnoSearch;


  public:
    NodeAnnoStorage(StringStorage& strings);
    NodeAnnoStorage(const NodeAnnoStorage& orig) = delete;

    template<typename Key, typename Value> using map_t  = bc::flat_map<Key, Value>;
    template<typename Key, typename Value> using multimap_t  = bc::flat_multimap<Key, Value>;

    using NodeAnnoMap_t = map_t<NodeAnnotationKey, uint32_t>;
    using InverseNodeAnnoMap_t = multimap_t<Annotation, nodeid_t>;


    void addNodeAnnotation(nodeid_t nodeID, Annotation& anno)
    {
      nodeAnnotations.insert(std::pair<NodeAnnotationKey, uint32_t>({nodeID, anno.name, anno.ns}, anno.val));
      inverseNodeAnnotations.insert(std::pair<Annotation, nodeid_t>(anno, nodeID));
      btree::btree_map<AnnotationKey, size_t>::iterator itKey = nodeAnnoKeys.find({anno.name, anno.ns});
      if(itKey == nodeAnnoKeys.end())
      {
         nodeAnnoKeys.insert({{anno.name, anno.ns}, 1});
      }
      else
      {
         itKey->second++;
      }
    }

    void addNodeAnnotationBulk(std::list<std::pair<NodeAnnotationKey, uint32_t>> annos);

    void deleteNodeAnotation(nodeid_t nodeID, AnnotationKey& anno)
    {
       auto it = nodeAnnotations.find({nodeID, anno.ns, anno.name});
       if(it != nodeAnnotations.end())
       {
          Annotation oldAnno = {anno.name, anno.ns, it->second};
          nodeAnnotations.erase(it);

          // also delete the inverse annotation
          inverseNodeAnnotations.erase(oldAnno);

          // decrease the annotation count for this key
          btree::btree_map<AnnotationKey, std::uint64_t>::iterator itAnnoKey = nodeAnnoKeys.find(anno);
          if(itAnnoKey != nodeAnnoKeys.end())
          {
             itAnnoKey->second--;

             // if there is no such annotation left remove the annotation key from the map
             if(itAnnoKey->second <= 0)
             {
                nodeAnnoKeys.erase(itAnnoKey);
             }
          }
       }
    }

    inline std::list<Annotation> getNodeAnnotationsByID(const nodeid_t &id) const
    {
      using AnnoIt =  NodeAnnoMap_t::const_iterator;

      NodeAnnotationKey lowerAnno = {id, 0, 0};
      NodeAnnotationKey upperAnno = {id, uintmax, uintmax};

      std::list<Annotation> result;
      std::pair<AnnoIt, AnnoIt> itRange = {
        nodeAnnotations.lower_bound(lowerAnno),
        nodeAnnotations.upper_bound(upperAnno)
      };

      for (AnnoIt it = itRange.first;
        it != itRange.second; it++)
      {
        const auto& key = it->first;
        result.push_back({key.anno_name, key.anno_ns, it->second});
      }

      return result;
    }

    inline std::pair<bool, Annotation> getNodeAnnotation(const nodeid_t &id, const std::uint32_t& nsID, const std::uint32_t& nameID) const
    {
      auto it = nodeAnnotations.find({id, nameID, nsID});

      if (it != nodeAnnotations.end())
      {
        return
        {
          true,
          {
            nameID, nsID, it->second
          }
        };
      }
      return
      {
        false,
        {
          0, 0, 0
        }
      };
    }

    inline std::pair<bool, Annotation> getNodeAnnotation(const nodeid_t &id, const std::string& ns, const std::string& name) const
    {
      std::pair<bool, std::uint32_t> nsID = strings.findID(ns);
      std::pair<bool, std::uint32_t> nameID = strings.findID(name);

      if (nsID.first && nameID.first)
      {
        return getNodeAnnotation(id, nsID.second, nameID.second);
      }

      std::pair<bool, Annotation> noResult;
      noResult.first = false;
      return noResult;
    }
    
    void calculateStatistics();
    bool hasStatistics() const;
    
    std::int64_t guessMaxCount(const std::string& ns, const std::string& name, const std::string& val) const;
    std::int64_t guessMaxCount(const std::string& name, const std::string& val) const;
    
    std::int64_t guessMaxCountRegex(const std::string& ns, const std::string& name, const std::string& val) const;
    std::int64_t guessMaxCountRegex(const std::string& name, const std::string& val) const;

    void clear();

    size_t estimateMemorySize();

    nodeid_t nextFreeID() const
    {
      return nodeAnnotations.empty() ? 0 : (nodeAnnotations.rbegin()->first.node) + 1;
    }

    virtual ~NodeAnnoStorage();

    template <class Archive>
    void serialize( Archive & ar )
    {
      ar(nodeAnnotations, inverseNodeAnnotations, nodeAnnoKeys, histogramBounds);
    }

  private:

    /**
     * @brief Maps a fully qualified annotation name for a node to an annotation value
     */
    NodeAnnoMap_t nodeAnnotations;
    InverseNodeAnnoMap_t inverseNodeAnnotations;

    /// Maps a distinct node annotation key to the number of keys available.
    btree::btree_map<AnnotationKey, std::uint64_t> nodeAnnoKeys;

    StringStorage& strings;
    
    /* additional statistical information */
    btree::btree_map<AnnotationKey, std::vector<std::string>> histogramBounds;
    
    
  private:
    /**
     * Internal function for getting an estimation about the number of matches for a certain range of annotation value
     * @param nsID The namespace part of the annotation key. Can be empty (in this case all annotations with the correct name are used).
     * @param nameID The name part of the annotation key.
     * @param lowerVal Inclusive starting point for the value range.
     * @param upperVal Inclusive end point for the value range.
     * @param if true upperVal is inclusive, otherwise it is exclusive
     * @return The estimation of -1 if invalid.
     */
    std::int64_t guessMaxCount(boost::optional<std::uint32_t> nsID, std::uint32_t nameID, const std::string& lowerVal,
      const std::string& upperVal) const;
  };
}


